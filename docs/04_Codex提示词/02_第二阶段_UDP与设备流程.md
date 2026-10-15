# Codex S2+S4：真实 UDP 收发与交互流程

以上阶段真实功能完成后，实现 Rust Tokio 的多 UDP endpoint/socket、收发实时日志、packet router、协议识别、PendingRequest 索引和定时任务；一个端点只绑定一个 socket，多个流程共享分发。发送前注册 pending，先 msgCode 再 txn+设备3B+端点+会话唯一匹配；处理重复应答、迟到、超时、重发（默认复用txn）、冲突和无序号协议降级串行。

使用 @xyflow/react 提供可序列化的 START/SEND/WAIT/SEND_AND_WAIT/ASSIGN/CONDITION/LOOP/PARALLEL/JOIN/END 等节点、右侧属性栏、超时分支、变量输出和执行轨迹；运行图时调用 Rust 执行器，UI 画布不承担异步通信逻辑。

完成真实收发页：日志过滤、HEX十六字节网格、中文字段解析树、1开始位置/0偏移切换、DEC/BIN/Bit拆解和HEX-字段双向高亮；未知报文保持HEX及“解析失败”。完成手工设备、UDP配置生成设备、五条逐项AA应用、设备三态、注册/认证流程和独立Token会话。

验收：本机双 UDP 端点回环，多设备同时注册乱序响应不串包，`0x22` 的 5 条 AA 与请求条目一一对应，发送成功不等于业务成功，超时重试和停止场景后无遗留 socket/task。提供集成测试和界面真实视频/截图说明（可用时）。
