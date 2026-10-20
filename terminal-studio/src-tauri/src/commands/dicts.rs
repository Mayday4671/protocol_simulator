//! 字典 Commands: 字典(dict_sets) → 条目(dict_items) 两层维护
//!
//! 字典 Key 限定 ASCII 标识符（模板中 `{dict:key}` 要用它）, 中文写在「名称」上。
//! 归属可选: 不选系统 = 全局共享; 选了系统则仅在该系统内可见, 并覆盖同 Key 的全局字典。

use tauri::State;

use crate::db;
use crate::engine::state::AppState;
use simterm_core::error::{AppError, AppResult};
use simterm_core::models::{DictItem, DictItemPayload, DictSetPayload, DictSetView};
use simterm_core::render::compact_hex;

/// 字典 Key 是内部标识({dict:key} 模板引用用), 界面不要求手填:
/// 留空自动生成; 手填时宽松规整(转小写/非法字符转下划线/数字开头补前缀), 不拦截报错
fn normalize_key(raw: &str) -> String {
    let key = raw.trim().to_lowercase();
    if key.is_empty() {
        return format!("d_{}", chrono::Utc::now().timestamp_millis());
    }
    let out: String = key
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() {
                c
            } else {
                '_'
            }
        })
        .collect();
    // 数字开头补前缀, 保证模板 {dict:key} 可解析
    if out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return format!("_{out}");
    }
    out
}

/// 全部字典（含条目）
#[tauri::command]
pub async fn list_dict_sets(state: State<'_, AppState>) -> AppResult<Vec<DictSetView>> {
    db::list_dict_sets(&state.pool).await
}

/// 新建（id 为 None）或修改字典
#[tauri::command]
pub async fn upsert_dict_set(
    state: State<'_, AppState>,
    id: Option<i64>,
    payload: DictSetPayload,
) -> AppResult<DictSetView> {
    let key = normalize_key(&payload.key);
    let name = payload.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::msg("字典名称不能为空"));
    }
    if !(1..=64).contains(&payload.byte_len) {
        return Err(AppError::msg(format!(
            "字典字节长度 {} 非法 (需 1-64)",
            payload.byte_len
        )));
    }
    // 指定了归属分组时先确认分组存在
    if let Some(gid) = payload.group_ref {
        db::get_group(&state.pool, gid).await?;
    }

    // 同作用域 Key 冲突: 自动追加序号(d_..._2), 不再让用户手工避让;
    // 编辑时 Key 未变(查到的就是自己)则原样通过
    let mut key = key;
    loop {
        let dup = db::find_dict_set(&state.pool, &key, payload.group_ref).await?;
        match dup {
            Some(d) if Some(d.id) == id => break,
            Some(_) => key = format!("{key}_2"),
            None => break,
        }
    }

    let normalized = DictSetPayload {
        key: key.clone(),
        name,
        system_ref: None,
        group_ref: payload.group_ref,
        byte_len: payload.byte_len,
        description: payload
            .description
            .map(|d| d.trim().to_string())
            .filter(|d| !d.is_empty()),
    };

    match id {
        None => db::insert_dict_set(&state.pool, &normalized).await,
        Some(id) => {
            let old = db::get_dict_set(&state.pool, id).await?;
            // 改 Key 会让已绑定的字段找不到字典, 有引用时直接拦住
            if old.key != key {
                let refs = db::dict_set_references(&state.pool, &old.key).await?;
                if !refs.is_empty() {
                    return Err(AppError::msg(format!(
                        "字典 Key \"{}\" 已被 {} 个协议引用（{}）, 不能改 Key; 请先解除这些字段的绑定",
                        old.key,
                        refs.len(),
                        refs.join(", ")
                    )));
                }
            }
            // 改字节长度必须与现有条目一致, 否则渲染时整帧会错位
            if old.byte_len != payload.byte_len {
                if let Some(bad) = old
                    .items
                    .iter()
                    .find(|i| i.value.len() != (payload.byte_len * 2) as usize)
                {
                    return Err(AppError::msg(format!(
                        "字典已有条目「{}={}」为 {} 字节, 与新长度 {} 字节不符; 请先调整条目",
                        bad.label,
                        bad.value,
                        bad.value.len() / 2,
                        payload.byte_len
                    )));
                }
            }
            db::update_dict_set(&state.pool, id, &normalized).await
        }
    }
}

/// 删除字典（含全部条目）; 仍被协议字段引用时拒绝
#[tauri::command]
pub async fn delete_dict_set(state: State<'_, AppState>, id: i64) -> AppResult<()> {
    let set = db::get_dict_set(&state.pool, id).await?;
    let refs = db::dict_set_references(&state.pool, &set.key).await?;
    if !refs.is_empty() {
        return Err(AppError::msg(format!(
            "字典「{}」正被 {} 个协议的字段引用（{}）, 删除后这些协议将无法渲染; 请先解除绑定",
            set.name,
            refs.len(),
            refs.join(", ")
        )));
    }
    db::delete_dict_set(&state.pool, id).await
}

/// 引用了该字典的协议 ID 列表（UI 展示引用数用）
#[tauri::command]
pub async fn dict_set_refs(state: State<'_, AppState>, id: i64) -> AppResult<Vec<String>> {
    let set = db::get_dict_set(&state.pool, id).await?;
    db::dict_set_references(&state.pool, &set.key).await
}

/// 新建（id 为 None）或修改条目; 值按字典字节长度校验
#[tauri::command]
pub async fn upsert_dict_item(
    state: State<'_, AppState>,
    id: Option<i64>,
    payload: DictItemPayload,
) -> AppResult<DictItem> {
    let set = db::get_dict_set(&state.pool, payload.set_ref).await?;
    let label = payload.label.trim().to_string();
    if label.is_empty() {
        return Err(AppError::msg("条目含义不能为空"));
    }
    let value = compact_hex(&payload.value);
    if value.is_empty() || hex::decode(&value).is_err() {
        return Err(AppError::msg(format!(
            "条目值必须是十六进制字节, 如 01 或 0A 00, 当前: {}",
            payload.value
        )));
    }
    let bytes = value.len() / 2;
    if bytes as i64 != set.byte_len {
        return Err(AppError::msg(format!(
            "字典「{}」每个条目为 {} 字节, 当前值 {} 是 {} 字节",
            set.name, set.byte_len, value, bytes
        )));
    }
    // 同字典内值不可重复（改条目时排除自身）
    if let Some(dup) = set.items.iter().find(|i| i.value == value) {
        if Some(dup.id) != id {
            return Err(AppError::msg(format!(
                "字典「{}」中已存在值 {}（{}）",
                set.name, value, dup.label
            )));
        }
    }
    let normalized = DictItemPayload {
        set_ref: payload.set_ref,
        value,
        label,
        sort_order: payload.sort_order,
    };
    db::upsert_dict_item(&state.pool, id, &normalized).await
}

#[tauri::command]
pub async fn delete_dict_item(state: State<'_, AppState>, id: i64) -> AppResult<()> {
    db::delete_dict_item(&state.pool, id).await
}
