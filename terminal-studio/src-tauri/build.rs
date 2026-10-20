fn main() {
    // Use the checked-in loader rather than a path into another checkout.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("gnu")
        && std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
    {
        let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").unwrap().as_str() {
            "x86_64" => "x64",
            "x86" => "x86",
            "aarch64" => "arm64",
            arch => panic!("Unsupported Windows architecture: {arch}"),
        };
        let loader = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
            .join("vendor/webview2-com-sys")
            .join(arch)
            .join("WebView2LoaderStatic.lib");
        println!("cargo:rerun-if-changed={}", loader.display());
        println!("cargo:rustc-link-arg={}", loader.display());
    }
    tauri_build::build()
}
