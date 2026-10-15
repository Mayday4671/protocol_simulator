use tauri::Manager;
pub mod application;
pub mod archive;
pub mod business;
pub mod call_cleanup;
pub mod domain;
pub mod flow_host;
pub mod network;
pub mod packet_business;
pub mod persistence;
pub mod project_io;
pub mod protocol;
pub mod runtime;
pub mod self_test;
pub mod simulator;
pub mod templates;
use std::sync::Arc;

#[tauri::command]
async fn studio(
    state: tauri::State<'_, Arc<application::Studio>>,
    request: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || tauri::async_runtime::block_on(state.request(request)))
        .await
        .map_err(|e| format!("后台任务失败: {e}"))?
}

pub fn run() {
    let mut context = tauri::generate_context!();
    if std::env::args().any(|a| a == "--inspect-local")
        && let Some(window) = context.config_mut().app.windows.first_mut()
    {
        window.additional_browser_args =
            Some("--remote-debugging-port=9223 --remote-debugging-address=127.0.0.1".into());
    }
    tauri::Builder::default()
        .setup(|app| {
            let data = app.path().app_data_dir()?;
            let (state, receiver) =
                application::Studio::open(&data).map_err(std::io::Error::other)?;
            state.start_receiver(receiver);
            state.start_scheduler();
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![studio])
        .build(context)
        .expect("MUSP Studio 无法启动")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::ExitRequested { .. }) {
                let state = app.state::<Arc<application::Studio>>();
                state.shutdown.cancel();
                tauri::async_runtime::block_on(state.udp.stop_all());
            }
        });
}
