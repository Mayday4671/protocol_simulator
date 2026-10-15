# MUSP Studio S0–S6 实施记录

**目标：** 按已冻结开发文档实现可安装 Windows 桌面程序，先 Rust 协议与真实 UDP，再业务与七页界面。

**架构：** React/Monaco/React Flow 仅编辑和呈现；Tauri IPC 调用 Rust Schema、Codec、UDP、流程和领域服务；SQLite WAL 保存本地项目、不可变发布版本、设备及分页日志。原始 docs 与图像不修改。

**技术栈：** Tauri 2 / Rust 2024 + Tokio + quick-xml + rusqlite；React 19 / TS 5 / AntD 5 / Zustand / TanStack Query & Virtual / xyflow 12 / Monaco。依赖由包管理器解析后锁定。

- [x] S0：`musp-studio` 工程、MSVC 工具链、SQLite 迁移、导航主题与项目；验证依赖安装、构建与迁移。
- [x] S1：`src-tauri/src/protocol` DSL/AST/表达式/Codec；验证原始生成脚本、99B/31B roundtrip、CRC/Bit/动态数组/恶意XML拒绝；界面草稿/验证/发布/复制与复用。
- [x] S2：`network` 多端点、后台接收、持久化、分页与解析；验证真实127.0.0.1双端点收发、坏包保留、停止释放端口。
- [x] S3：`domain` 设备、配置映射、注册会话；验证仅三态、逐AA、已有设备配置不改状态。
- [x] S4：`runtime` 图执行与PendingRequest；验证先登记后发包、唯一匹配、乱序并发、超时重试/取消、分支/循环/并行/子流程。
- [x] S5：通话/RX/TX资源、可配置启动方式、结束/掉线；验证多通话隔离、UNKNOWN不回池、幂等释放。
- [x] S6（本机验收，干净虚拟机验收未执行）：模板/导出/故障注入/用户文档、完整测试、NSIS offlineInstaller 实际构建与安装/功能/卸载验证。

测试命令和结果逐阶段写入 `docs/TEST_REPORT.md`；任何尚未运行验收不标通过，业务歧义写入 `docs/OPEN_QUESTIONS.md`。


