# 收发时间轴与选择框对齐

用户要求：通道标签与其他选择框对齐；注销等交互必须显示发送请求、等待响应两步，而非把它们压成一个节点。

实现：五个筛选项统一 Label 和独立选择框结构。时间轴把配置了期待响应的事务展开为发送、响应两个真实进度节点，各自展示自己的事件和报文；发送完成不代表业务响应成功。底层保留既有事务关联与日志边界，避免将响应拆为无请求关联的被动接收而误匹配或漏掉快速应答。单向发送和被动接收保持一个节点。

- [x] 发送/响应状态：未发送、已发送等待响应、成功、拒绝、超时、停止、新轮次与报文归属测试。
- [x] 抽屉与自动关联 UI 回归、TypeScript、前端构建。
- [x] 最终 Windows EXE：五个选择框对齐，六种交互各两节点，发送后等待、业务拒绝及超时真实 UDP 验证；深浅色与 1024×768 布局检查。
- [x] NSIS 安装包。

`test_flow_timeline.cjs`、`ui_flow_drawer.cjs`、`ui_protocol_flow_link.cjs`、TypeScript、Vite 和 release 构建通过。`native_iot_independent_flows.cjs` 在隔离数据库与 UDP 对端（2200ms 响应延迟）中验证六种独立交互；重复注销收到 03 拒绝，缩短隔离流程期限后验证响应超时。两种情况下发送节点均保持已发送。测试结束恢复隔离流程配置。本次未修改用户数据、未覆盖运行中的客户端。

安装包：`release/Simulated-Terminal_0.1.0_exchange-timeline_20261022_x64-setup.exe`。
SHA256：`7BAA933DE6E983BDC6A217809FE5C00880E10B7564AC7D828C4621440BD3B25F`。
安装器已生成，未执行本次安装验收。截图位于 `artifacts/iot-independent-flows/`。
