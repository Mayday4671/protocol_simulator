use tauri::Manager;
pub mod application;
pub mod archive;
pub mod business;
pub mod call_cleanup;
pub mod domain;
pub mod flow_host;
pub mod flow_preview;
mod launch_options;
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
    let options =
        launch_options::LaunchOptions::parse(std::env::args_os().skip(1)).expect("无效启动参数");
    let mut context = tauri::generate_context!();
    if options.inspect_local
        && let Some(window) = context.config_mut().app.windows.first_mut()
    {
        window.additional_browser_args =
            Some("--remote-debugging-port=9223 --remote-debugging-address=127.0.0.1".into());
        if let Some(data) = &options.test_workspace {
            window.data_directory = Some(data.join("webview"));
            window.title = "MUSP Studio · 独立测试".into();
            // The config adapter does not preserve an absolute WebView data
            // directory; set it explicitly on the builder in setup instead.
            window.create = false;
        }
    }
    tauri::Builder::default()
        .setup(move |app| {
            let data = options
                .test_workspace
                .clone()
                .map_or_else(|| app.path().app_data_dir(), Ok)?;
            let (state, receiver) =
                application::Studio::open(&data).map_err(std::io::Error::other)?;
            state.start_receiver(receiver);
            state.start_scheduler();
            app.manage(state);
            if options.test_workspace.is_some() {
                let config = app.config().app.windows.first().ok_or("缺少主窗口配置")?;
                tauri::WebviewWindowBuilder::from_config(app.handle(), config)?
                    .data_directory(data.join("webview"))
                    .build()?;
            }
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
