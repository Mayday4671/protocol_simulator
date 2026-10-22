use tauri::{AppHandle, Emitter, State};
use crate::engine::state::AppState;
use simterm_core::error::AppResult;
pub use crate::devices::TerminalProfile;

#[tauri::command]
pub async fn list_terminals(state: State<'_, AppState>) -> AppResult<Vec<TerminalProfile>> {
    Ok(sqlx::query_as("SELECT * FROM terminal_profiles ORDER BY id").fetch_all(&state.pool).await?)
}

#[tauri::command]
pub async fn save_terminal(app: AppHandle, state: State<'_, AppState>, payload: TerminalProfile) -> AppResult<()> {
    crate::db::get_system(&state.pool, payload.system_ref).await?;
    crate::devices::save_profile(&state.pool, payload).await?;
    let _ = app.emit("devices-updated", ());
    Ok(())
}

#[tauri::command]
pub async fn delete_terminal(app: AppHandle, state: State<'_, AppState>, id: i64) -> AppResult<()> {
    let mut tx = state.pool.begin().await?;
    let p: TerminalProfile = sqlx::query_as("SELECT * FROM terminal_profiles WHERE id=?").bind(id).fetch_one(&mut *tx).await?;
    let linked:i64=sqlx::query_scalar("SELECT COUNT(*) FROM business_records WHERE device_ref=?").bind(id).fetch_one(&mut *tx).await?;
    if linked>0{return Err(simterm_core::error::AppError::msg("该设备仍有关联业务资料，请先删除关联资料"));}
    sqlx::query("DELETE FROM business_records WHERE node_ref=? AND system_ref=? AND kind='device' AND record_key=?")
        .bind(p.node_ref).bind(p.system_ref).bind(&p.device_key).execute(&mut *tx).await?;
    let raw: Option<String> = sqlx::query_scalar("SELECT captures_json FROM nodes WHERE id=?").bind(p.node_ref).fetch_one(&mut *tx).await?;
    let mut buckets: serde_json::Value = serde_json::from_str(raw.as_deref().unwrap_or("{}")).unwrap_or(serde_json::json!({}));
    if let Some(obj) = buckets.as_object_mut() { obj.remove(&format!("s{}:{}",p.system_ref,p.device_key)); }
    sqlx::query("UPDATE nodes SET captures_json=? WHERE id=?").bind(buckets.to_string()).bind(p.node_ref).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM terminal_profiles WHERE id=?").bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    state.engine.pending_configs.retain(|k,_| !(k.node==p.node_ref && k.system==p.system_ref && k.device==p.device_key));
    let _ = app.emit("devices-updated", ());
    Ok(())
}
