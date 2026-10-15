#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_some_and(|v| v == "--self-test") {
        let Some(path) = args.get(2) else {
            std::process::exit(2)
        };
        let path = std::path::Path::new(path);
        let runtime = tokio::runtime::Runtime::new().expect("无法创建异步运行时");
        let result = runtime.block_on(musp_studio::self_test::run(path));
        let ok = result.is_ok();
        let value =
            result.unwrap_or_else(|error| serde_json::json!({"passed":false,"error":error}));
        if std::fs::create_dir_all(path)
            .and_then(|()| {
                std::fs::write(
                    path.join("self-test.json"),
                    serde_json::to_vec_pretty(&value).unwrap(),
                )
            })
            .is_err()
        {
            std::process::exit(3);
        }
        std::process::exit(if ok { 0 } else { 1 });
    }
    musp_studio::run()
}
