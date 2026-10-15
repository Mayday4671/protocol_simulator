# MUSP Studio

MUSP Studio 是 Windows 10/11 x64 上的本地二进制 UDP 协议工作台。React 界面通过 Tauri IPC 调用 Rust：XML 编译、报文编解码、CRC、UDP 收发、流程定时、设备与通话状态和 SQLite 持久化均由后端执行。无需部署 Java、Redis 或数据库服务。

当前工程版本为 **0.1.0**。发行包安装、实际操作与自测见 [安装使用说明](release/README_安装使用.md)。安装验收结果以仓库 [测试报告](../docs/TEST_REPORT.md) 为准；配置了 WebView2 离线安装模式不等于完成了干净虚拟机离线验收。

## 已实现的工作区

| 业务页 | 可执行操作 |
|---|---|
| 工作台 | 项目概览、真实 UDP 监听端点、设备状态及流程实例 |
| 协议设计 | XML 草稿、共享结构与字段编辑、数组和 Bit 属性、校验、不可变发布版 |
| 设备管理 | 系统归属、3B 协议地址、内部 UUID、参数编辑及三态显示 |
| 交互流程 | 画布编排、节点 JSON、条件/循环/并行/子流程、发布、运行、停止与轨迹 |
| 报文监控 | Rust 编码、解析、发送一次、真实 RX/TX、HEX 与字段定位、JSON/JSONL/CSV 导出 |
| 通话资源 | 通话参与方、RX/TX 独立实例、资源池、结束清理及 UNKNOWN 人工核实 |
| 模板库 | 七个可编辑业务流程模板、协议版本选择与 XML 编辑 |

另有“系统设置”入口，管理项目 ZIP 导入导出、参与系统、数据目录、监听端点及心跳超时。

模板不会自动制造在线设备或通话成功结果。配置模板必须收到正确的 `0x22`；注册必须完成匹配的许可及完成响应；通话模板需要已注册设备、实际资源池和真实申请报文。内置本地对端是显式启用的 Rust UDP 应答器，收发同样经过操作系统 socket。

## 开发与构建

技术栈遵循 [工程规范](../docs/03_开发与测试/06_技术架构与工程规范.md)：Tauri 2、React 19、TypeScript 5、Vite 6、Ant Design 5、Zustand 5、TanStack Query / Virtual、React Flow 12、Monaco；后端为 Rust edition 2024、Tokio、quick-xml、serde 与 rusqlite（bundled SQLite）。具体依赖由 `pnpm-lock.yaml`、`src-tauri/Cargo.lock` 锁定。

本次文档核对环境：Node.js `22.16.0`，pnpm `10.24.0`。Rust 版本以 [rust-toolchain.toml](rust-toolchain.toml) 中的 `1.97.1-x86_64-pc-windows-msvc` 为准。Windows 开发机需要对应 MSVC C++ 构建工具、Windows SDK 与 WebView2；首次下载依赖和打包运行时可能需要网络。

在 `musp-studio` 目录运行：

```powershell
pnpm install --frozen-lockfile
pnpm desktop
```

`pnpm dev` 只启动 Vite；普通浏览器不会具备 Tauri 后端，因此不能用它验证真实 UDP 或持久化。`pnpm desktop` 启动完整桌面开发程序。

```powershell
pnpm build
pnpm test
cargo test --manifest-path src-tauri/Cargo.toml --locked
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
pnpm package:windows
```

这些是可复现的检查/打包命令，不代表此文档已宣称每项通过。NSIS 产物位于 `src-tauri/target/release/bundle/nsis/`；发布时将实际 Setup.exe、SHA256 清单与使用说明整理到 `release/`。打包配置使用当前用户安装及 WebView2 `offlineInstaller`，目前未配置代码签名。

## 工程入口

| 目录/文件 | 内容 |
|---|---|
| `src/api/tauri.ts` | 前端 IPC 封装 |
| `src/features/` | 七个业务页面及系统设置 |
| `src-tauri/src/protocol/` | XML DSL、编译、编码/解码与 CRC |
| `src-tauri/src/network/` | Tokio UDP 与有界接收队列 |
| `src-tauri/src/runtime/` | 流程执行、PendingRequest 索引、超时与重试 |
| `src-tauri/src/domain/` | 配置、设备三态、会话、通话与资源规则 |
| `src-tauri/src/flow_host.rs` | 真实发送、匹配、业务提交与执行动作适配 |
| `src-tauri/src/persistence/`、`migrations/` | SQLite 查询、持久化与迁移 |
| `src-tauri/src/templates.rs`、`workflows/` | 七个内置流程的安装与 JSON 定义 |
| `protocols/musp-v08.xml` | 从原始业务基线整理的可编辑协议样例 |
| `src-tauri/src/self_test.rs` | 安装后隔离目录自测 |

设备只显示 `OFFLINE / ONLINE / COMMUNICATING`。配置不重置已注册设备状态。响应匹配包含端点、协议版本、远端、源/目标地址、事务、会话及必要业务键；发送前登记等待，重复或迟到确认不能重复结算。资源按通话、设备、方向与版本隔离，释放未确认保持 `UNKNOWN`，不自动归还池容量。

关闭后再启动不会自动重发上次运行。会话需要重新建立；未确认资源恢复为 UNKNOWN，等待真实确认或人工核实。项目和报文均保存在“系统设置”显示的本地数据目录；项目 ZIP 与报文 JSONL 分别导出，ZIP 不是完整数据库备份。

## 业务边界与声明

原始 [MUSP v0.8 业务基线](../docs/07_原始协议讨论稿/MUSP_v0.8_协议业务基线_原文不修改.md) 保持原文。`configCommon8`、配置生效方式等仍有待业务方确认的解释，见 [待确认问题](../docs/OPEN_QUESTIONS.md) 与安装说明。示例频率、带宽、地址和有限心跳次数可编辑，不构成现场设备配置建议。

注册提供 `authMode=0` 与独立认证下发两个模板。认证模板等待真实 `0x11`，实际发送 `0x12` 后使用会话 Token 发送带认证扩展的 `0x03`；需要支持这套交互的外部控制端，内置本地应答器默认使用无需认证模式。设备 A/B 可通过 `0x73/0x74` 发起当前通话的结束，尚未建立的申请可通过 `0x7B/0x7C` 取消；受理不等于资源释放确认。真实设备兼容性和干净机器安装应按部署环境验收。

依赖来源、许可证元数据与可取得的原始声明见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。变更记录见 [CHANGELOG.md](CHANGELOG.md)。
