# 多协议模拟系统

Windows 桌面应用，基于 Tauri 2、React 19、TypeScript、Tailwind/shadcn-ui 和 Rust（Tokio + SQLx）。

## 当前功能

- 通道管理：配置本地/对端 UDP 地址、启动监听和真实收发。
- 协议管理：XML 编辑、字段和字典、多个公共头选择与拼接、条件字段、动态数组、长度及 CRC 计算。
- 设备与业务流程：设备参数、发送、被动接收、响应取值及流程运行。
- 交互控制台与监控：发送表单、实时收发记录、按公共头/净荷/尾部展示解析结果，支持 HEX、十进制、中文含义和 Bit 信息。
- 总览：通道状态、今日收发及历史趋势/通道统计。

业务规则以仓库根目录的 `docs` 为准，已确定规则与待确认项分别记录。示例及联调限制见 [MUSP 试录说明](docs/MUSP_v0.8_基座试录差异.md)。

## 开发

Windows 环境准备 Node.js、Rust、WebView2 Runtime。执行：

```powershell
npm ci
npm run tauri dev
```

生成 NSIS 安装程序：

```powershell
npm run tauri build
```

产物目录：`src-tauri/target/release/bundle/nsis/`。本机 GNU 链接设置位于 `.cargo/config.toml`，WebView2 所需库位于工程内的 `src-tauri/vendor`，无需旧基座目录。

## 测试与联调

```powershell
cd src-tauri
cargo test -p simterm-core
```

核心测试覆盖协议编解码、匹配、公共头、动态数组、报文分区和统计。`scripts/` 保留界面回归脚本及模拟对端；浏览器检查需要 Playwright/Chrome，实际 UDP 联调见 [模拟对端使用](docs/模拟对端使用.md)。不以大包速率压测代替功能验证。

应用壳在 Windows GNU 下的单元测试宿主可能缺少 Common Controls manifest，核心测试与实际 EXE 验收分开记录。

## 使用与数据

- [使用手册](docs/使用手册.md)
- [公共头组合](docs/公共头组合与窗口整合验收.md)
- [报文分区](docs/报文分区.md)
- `protocols/`：通用、无人机及其他测试协议。

运行数据库默认位于 `%APPDATA%/com.simterm.desktop/simterm.db`。数据库、WebView 缓存、预览目录、测试截图、构建缓存和安装包不提交 Git。

原始基座来自用户指定的 `E:/My_Project/Simulated_Terminal`；该目录仅为来源记录，不是构建依赖。
