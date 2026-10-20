//! 多协议 UDP 终端模拟系统 - Tauri 后端装配

mod commands;
pub mod devices;

#[cfg(feature = "e2e")]
pub use commands::protocols::validate_all_protocols as e2e_validate_all_protocols;
#[cfg(feature = "e2e")]
pub mod db;
#[cfg(not(feature = "e2e"))]
mod db;
#[cfg(any(test, feature = "e2e"))]
pub mod engine;
#[cfg(not(any(test, feature = "e2e")))]
mod engine;
mod webview2_shim;
#[cfg(test)]
mod header_tests;

pub use engine::state::{AppState, EngineState};

use std::sync::Arc;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut context = tauri::generate_context!();
    // Explicit local UI-test opt-in; normal launches do not expose a debug port.
    if let Some(port) = std::env::var("SIMTERM_UI_TEST_PORT").ok().and_then(|v| v.parse::<u16>().ok()).filter(|p| *p > 0) {
        for window in &mut context.config_mut().app.windows {
            let args = window.additional_browser_args.get_or_insert_with(String::new);
            args.push_str(&format!(" --remote-debugging-address=127.0.0.1 --remote-debugging-port={port}"));
        }
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let pool = tauri::async_runtime::block_on(db::init_pool(&handle))
                .map_err(|e| format!("初始化数据库失败: {e}"))?;
            app.manage(AppState {
                pool: pool.clone(),
                engine: Arc::new(EngineState::new()),
            });
            commands::flows::spawn_flow_scheduler(app.handle().clone(), pool.clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::terminals::list_terminals,
            commands::terminals::save_terminal,
            commands::terminals::delete_terminal,
            // 节点
            commands::nodes::list_nodes,
            commands::nodes::create_node,
            commands::nodes::update_node,
            commands::nodes::delete_node,
            // 协议
            commands::protocols::list_protocols,
            commands::protocols::upsert_protocol,
            commands::protocols::delete_protocol,
            commands::protocols::import_protocols_from_xml,
            commands::protocols::export_protocols_to_xml,
            commands::protocols::import_protocols_xml_text,
            commands::protocols::export_protocols_xml_text,
            commands::protocols::list_systems,
            commands::protocols::create_system,
            commands::protocols::update_system,
            commands::protocols::update_group_header,
            commands::protocols::update_group_headers,
            commands::protocols::delete_system,
            commands::protocols::list_groups,
            commands::protocols::create_group,
            commands::protocols::update_group,
            commands::protocols::delete_group,
            // 字典（字典 → 条目）
            commands::dicts::list_dict_sets,
            commands::dicts::upsert_dict_set,
            commands::dicts::delete_dict_set,
            commands::dicts::dict_set_refs,
            commands::dicts::upsert_dict_item,
            commands::dicts::delete_dict_item,
            // 引擎
            commands::engine_cmds::start_udp_server,
            commands::engine_cmds::stop_udp_server,
            commands::engine_cmds::engine_status,
            commands::engine_cmds::send_udp_message,
            commands::engine_cmds::login,
            commands::engine_cmds::start_periodic_send,
            commands::engine_cmds::stop_periodic_send,
            commands::engine_cmds::list_heartbeats,
            commands::engine_cmds::upsert_heartbeat,
            commands::engine_cmds::delete_heartbeat,
            commands::engine_cmds::start_heartbeat_task,
            commands::engine_cmds::stop_heartbeat_task,
            commands::engine_cmds::stop_all,
            // 业务流程
            commands::flows::list_flows,
            commands::flows::upsert_flow,
            commands::flows::delete_flow,
            commands::flows::start_flow_run,
            commands::flows::stop_flow_run,
            commands::flows::flow_runs,
            // 日志
            commands::logs::query_logs,
            commands::logs::clear_logs,
            commands::logs::log_stats,
            commands::logs::traffic_stats,
            commands::logs::parse_datagram,
        ])
        .run(context)
        .expect("error while running tauri application");
}
