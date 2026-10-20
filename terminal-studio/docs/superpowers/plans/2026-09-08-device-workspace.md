# Device Workspace Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use subagent-driven-development or executing-plans. 当前目录无 Git，不创建仓库、分支或提交；按文件边界协作。

**Goal:** 打通统一设备档案、配置确认入库和指令复用，同时交付保状态标签页及默认不溢出的字典页面。

**Architecture:** 使用现有 terminal_profiles；协议字段新增 profile_key，关联流水号使用 correlation 角色。纯映射逻辑放在 core/profile.rs，持久化及待确认快照放在桌面端设备服务。前端统一设备表单，控制台按映射预填，导航外壳保留已打开页面实例。

**Tech Stack:** Rust / SQLite / Tauri / React / TypeScript / Playwright。

## Task 1: 工作区标签与自适应字典（独立子任务）

Files: src/App.tsx、src/components/layout/WorkspaceTabs.tsx、src/pages/DictionaryManager.tsx、src/components/DictionarySection.tsx、局部必要 CSS；scripts/ui_workspace_regression.cjs。

- [ ] 先写 Playwright 用例：点击协议页输入搜索/草稿，切到字典再回来，断言值保留；同一路径一标签；关闭当前标签选邻近页。
- [ ] 在 1700×1075 与 1366×768 断言折叠字典列表 scrollHeight <= clientHeight；翻页可遍历所有字典；展开 96 项字典仍可翻完条目。
- [ ] 运行得到 RED，再实现 openedPaths + active pathname 的受控 tablist；每个页面实例各自持久挂载，隐藏页不可交互；局部 ResizeObserver 分页。
- [ ] 运行 npm run build 和 UI 用例得到 GREEN，再规格/质量复核。

## Task 2: 显式设备属性与后台存储

Files: src-tauri/core/src/models.rs、xml.rs、profile.rs、lib.rs；src-tauri/src/devices.rs、commands/terminals.rs、engine/udp.rs、engine/state.rs、lib.rs；src/types/index.ts。

- [ ] 先写纯 Rust 用例：profile_key="zone" 的字段改显示名后仍正确取值；没有 profile_key 的业务域和消息码不被设备数据覆盖；禁止将 TypeCode/Session/Correlation 标为普通属性。
- [ ] 写身份编码、属性值宽度/字典、零列表、运行时字段拒绝测试，运行 RED。
- [ ] 增加字段 `#[serde(default, skip_serializing_if="Option::is_none")] pub profile_key: Option<String>`，角色 `Correlation`，XML 属性 profile-key 往返不丢失。
- [ ] 实现纯函数：映射字段选择、属性收集、按当前帧预填、身份取值；按真实字段编码校验，未知属性拒绝保存。
- [ ] 把保存/删除服务与 Tauri 命令包装分离；保存不接收客户端 Session 更新，按身份和系统检查通道与配置；发设备变更事件。
- [ ] 发送时读取最新档案，合并属性和显式参数，仅覆盖选中设备身份；Session 按原独立捕获桶取值。

## Task 3: 配置应答与设备管理

Files: src-tauri/src/devices.rs、engine/udp.rs、engine/state.rs；src/pages/DeviceManager.tsx、components/TerminalProfiles.tsx、lib/deviceProfiles.ts、lib/api.ts、pages/SimulationConsole.tsx、components/protocols/ListEntries.tsx；协议字段编辑器相关文件。

- [ ] 写待确认配置测试，匹配 key 为 `(node,system,response_protocol,correlation,device)`；成功提交，失败/重复/过期不提交。
- [ ] 发送前登记配置快照，发送失败撤销；上限和过期回收；收到成功结果时原子保存设备属性并通知界面。
- [ ] 去掉设备管理中的第二份“序列号字典档案”界面；设备列表分页和动态表单共用同一份 profiles 数据。
- [ ] 设备表单使用字段映射和字典，复用 FieldBranches/ListEntries；列表支持从已存 Hex 恢复记录，不能把已保存配置重置为单位默认值。
- [ ] 控制台按当前设备、协议独立保存/重置临时参数；本次业务参数不回写设备，配置确认才更新；订阅设备变化事件。
- [ ] 协议编辑器能配置属性 Key，XML 导入导出保留该元数据。

## Task 4: 数据、集成验收与交付

Files: scripts/iot_seed_data.py、scripts/import_xml.py、scripts/device_workspace_dataset.py、scripts/test_device_workspace_dataset.py、scripts/ui_device_workspace.cjs；src-tauri/src/bin/e2e_device_workspace.rs；dist_portable。

- [ ] 生成三族映射配置，建出有不同编号、类型、区域与附属列表的样例设备；只在隔离库运行真实 UDP。
- [ ] 回归：75 项原 core 测试、旧 699 帧样例、设备建档/编辑/切换/配置确认及 Session 隔离。
- [ ] 按用户最新明确指示，直接事务清空全部旧模拟业务数据并重建完整示例，不做旧数据迁移或隔离；核对重建数量和完整性。
- [ ] 构建独立 EXE 并更新原打包目录，XML 同步更新，不创建压缩包；验证文件哈希与真实页面。

## Commands

```powershell
cargo test -p simterm-core
cargo test --lib devices
cargo run --features e2e --bin e2e_device_workspace -- E:/My_Project/Simulated_Terminal/artifacts/device-workspace-validation.db
npm run build
node scripts/ui_workspace_regression.cjs
node scripts/ui_device_workspace.cjs
```

生产数据更改和 EXE 发布都在最终验收通过之后。运行用户自己的进程时不强制结束；如锁定旧发布文件，保留原件并输出旁置新文件。
