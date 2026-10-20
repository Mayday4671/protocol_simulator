# MUSP Studio / 多协议模拟系统

当前 Windows 桌面工程位于 [`terminal-studio`](terminal-studio/README.md)，基于 Tauri 2、React、TypeScript 和 Rust。保留用户指定基座的操作方式，实现 UDP 收发、协议编辑与解析、公共头组合、设备及业务流程、通信统计。

## 目录

- `terminal-studio/src`：客户端界面。
- `terminal-studio/src-tauri`：Rust 通信与协议引擎、SQLite、原生窗口和 NSIS 配置。
- `terminal-studio/protocols`：可导入的示例协议及测试数据。
- `terminal-studio/scripts`：模拟对端、数据准备及回归检查。
- `terminal-studio/docs`：当前操作手册和协议使用说明。
- `docs`：原始需求、协议规则、七张原型图、测试记录及待确认事项。

## 开发与构建

在 Windows 安装 Node.js、Rust 和 WebView2 Runtime，然后在 `terminal-studio` 下运行：

```powershell
npm ci
npm run tauri dev
```

生成 Windows NSIS 安装程序：

```powershell
npm run tauri build
```

产物位于 `terminal-studio/src-tauri/target/release/bundle/nsis/`。本机采用 GNU Rust 工具链，链接配置位于 `terminal-studio/.cargo/config.toml`；WebView2 加载库随工程保存，不依赖旧基座目录。

核心测试：

```powershell
cd terminal-studio/src-tauri
cargo test -p simterm-core
```

## 数据与历史

运行数据库默认位于 `%APPDATA%/com.simterm.desktop/simterm.db`，不纳入 Git。原始协议业务规则及未确定项以 `docs` 为准。

当前 Git 工作树已移除被替换的 `musp-studio`，旧版本备份、临时预览和生成的测试产物不纳入仓库。历史测试报告保留为过程记录，其中旧安装包和截图路径不属于当前交付；旧版已提交源码仍可通过 Git 历史查阅。当前工程不再依赖这些旧目录。
