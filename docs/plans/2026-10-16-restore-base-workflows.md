# 恢复基座操作路径

用户已明确选择 E:/My_Project/Simulated_Terminal 为基座，并要求立即修正全部主要页面。此次按原页面的操作顺序、控件和信息密度接入已完成的 XML/Rust 引擎，不再将换色或删标题视为完成。

技术栈、协议数据及真实 UDP 引擎不变。禁止切到旧数据库来伪装兼容，禁止改写无人机报文字节规则。

## 工作项与边界

- [ ] 控制台：参照原 SimulationConsole，顶部集中协议/信令/通道/目标；公共头折叠，业务字段充分利用宽度；一键编码并发送，编码预览可选；按钮受阻时显示原因和可操作入口；底部收发记录默认紧凑，可折叠，解析按需展开而非固定占半屏。每信令草稿/配置隔离继续有效。文件 Console.tsx、Packets.tsx、HeaderParameters.tsx、console-restored.css。
- [ ] 协议/字典：参照原 ProtocolManager、GroupHeaderCard、FieldRowItem，恢复紧凑列表和字段行；公共头独立选择，信令组合只展示有用信息；枚举用选择项、技术属性中文标识；不重复每行卡片/工具栏/说明；数组与位域可展开编辑，XML保留高级入口。文件 Protocols.tsx、ProtocolStructure.tsx、HeaderLibrary.tsx、MessageComposition.tsx、header-editor.css。
- [ ] 流程：参照原 FlowManager，流程列表与选中流程同页；常用动作/保存/运行就近；新建发送/等待节点可从可用信令选择，节点配置保留类型和明确来源；缺通道可直接处理或跳转。保留画布、运行实例、快照、实时收发和真实引擎。文件 Flows.tsx、flow-restored.css。
- [ ] 根协调：通道/设备/总览/页面壳统一基座控件和信息密度；恢复清晰选中、创建/编辑、持久保存与返回路径，隐藏不相关高级信息。文件 Workspace.tsx、Devices.tsx、workbench.css、context.tsx。
- [ ] 验证：先静态与单元测试，再客户端正常速率实际UDP和无人机业务回归；按新用户路径从选协议到发送/接收运行，不直接注入状态。保存关键页面截图，比较真实控件和视口，不以测试数量代替体验。
- [ ] 打包独立新版本。原安装运行中时不强制关闭；先完成可审阅安装包，再处理隔离安装验收。

采用 subagent-driven-development：独立文件任务分工，根协调做接口/一致性审查与最终验收。所有修改限 terminal-studio；原基座仅作读取参考。工作区已有未提交工作，不重置、不提交无关文件。
