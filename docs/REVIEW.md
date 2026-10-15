# MUSP Studio 协议、安全与持久化审查记录

记录日期：2026-10-15。范围：Rust XML DSL/codec、真实 UDP、协议关联与 Token 隔离、项目导入导出、SQLite 报文存储，以及本次安装交付前的局部审查。

本文区分实际执行的测试、代码审查和未覆盖项。原始协议讨论稿及需求文档未修改。这里的测试数量对应执行时的代码状态，不代替主交付流程随后运行的全套测试、安装包构建或干净机器验收。

**用户已明确取消高频压力验收：1000 包/秒持续 300 秒及 5000 包/秒持续 60 秒不再作为交付门槛。** 已停止长测，保留既有结果，不声称达到高吞吐或零丢包。正常协议、安全与业务回归继续有效。

## 1. 实际执行结果

环境为 Windows 原生开发工作区，Rust 项目使用 `x86_64-pc-windows-msvc`。测试使用临时 SQLite 数据库；涉及收发的回归使用真实本机 UDP socket。

| 命令／范围 | 实际结果 | 证据与时间边界 |
| --- | --- | --- |
| `cargo test --test protocol_golden` | 29/29 通过 | [协议回归源码](../musp-studio/src-tauri/tests/protocol_golden.rs)，包含示例及原文报文字节往返；随后未修改协议实现 |
| `cargo test --test application_security --test udp_transport --test persistence_storage` | 14 + 2 + 2，共 18/18 通过，退出 0 | 最后一次局部修复后的运行：安全约 0.86 秒、存储约 0.11 秒；含 Windows UDP 端口关闭恢复、坏包脱敏新增案例 |
| `cargo test --tests` | 当时全套 88/88 通过，退出 0 | 在最后新增 Windows UDP 回归之前执行；不能当作当前最终版本的测试数量 |
| `cargo clippy --all-targets -- -D warnings` | 退出 0 | 最后一次 network/application 局部修复后执行，约 6.10 秒；后续其他分工修改应由交付流程重新验证 |

历史全套 88 项的组成是 lib 18、application_security 13、application_udp 2、call_udp 1、concurrent_registration 1、flow_runtime 17、persistence 1、persistence_storage 2、protocol_golden 29、udp_transport 2、workflow_udp 2。其中并发注册测试使用真实 UDP、100 个设备和两个隔离控制端，执行约 9.02 秒。后续新增测试没有倒填到该历史结果。

以上命令实际输出已在开发会话中检查；不是根据测试文件存在推断通过。源码链接方便复核断言，不代表每次终端输出均另存成了日志文件。本轮整理文档没有重新编译或运行压力测试。

在 `musp-studio/src-tauri` 目录可复核本分工的普通回归：

```powershell
cargo test --test protocol_golden --test application_security --test udp_transport --test persistence_storage
cargo clippy --all-targets -- -D warnings
```

## 2. 协议与 DSL：29 项回归

实现使用 quick-xml 解析为可序列化的强类型 AST。同一个已编译 schema 驱动 encode/decode，支持嵌套 struct、fragment 引用、动态数组、变体、条件字段、大小端整数、原始字节、ASCII、BCD、Bit 及中文枚举解释。长度、数组计数、Token 长度及 CRC 在编码时按 schema 计算；解码生成逐字段及逐 Bit 的偏移信息。

固定业务规则保持原文约束：基础包头 23B、信令码 1B；源/目的地址各 3B、会话号 2B；0x21 的可选公共段 8B，配置前缀 10B，再按数量和 itemSize 读取；0x22 是数量 1B 加逐项结果各 1B，AA 表示成功，未知值保留原 HEX，不补造错误码。

| 验证主题 | 实测覆盖 |
| --- | --- |
| 示例与原文 | `musp-demo.xml` 编译；99B/31B golden decode→encode 逐字节相同；99B 配置请求 CRC D35D、31B 响应 CRC FECB；原文 99B CRC EB46 及 35B 注册、32B 许可、43B 视频、39B 语音样例往返 |
| 完整模板 | `musp-v08.xml` 的 34 条信令定义可编译；测试覆盖上述原文样例，不把“全部定义编译”表述为每条信令均完成外部设备互通 |
| 字段显示 | 中文枚举、Bit 分解、嵌套数组、字节跨度和位跨度准确；未知结果保留原值 |
| 计算与条件 | 修改数组数量后同步重算 body/header/packet 长度及 CRC；关闭可选公共段；signed little-endian 与 unsigned 共用 codec；修改 Bit 保留其他位 |
| 嵌套作用域 | 内层数组 count 不污染外层长度；struct 局部作用域；完整路径及无歧义 fragment 限定路径引用 |
| 发布前拒绝 | 重复字段 key、循环 fragment、Bit 重叠、默认 itemSize 与变体不符、`len()` 引用不存在字段、前向长度引用、字节值误作数值、未守卫的条件字段引用、移动信令码位置的可选基础头 |
| 不可信 XML／表达式 | DTD、实体声明、未知元素／属性、任意函数调用、拼写错误的约束均拒绝；错误带位置，不静默忽略规则 |
| 坏包 | 长度错误、CRC 错误、数组越界、恶意长度及 golden 包的每个截断前缀返回有界错误 |
| secret | 可按 span 遮蔽字节；secret 字段及子 Bit 的解释不带原值；范围和解析失败诊断不回显 Token |

限制在实现中显式设定：XML 1MiB、包长 65507B、XML 深度 32／节点 10000、引用深度 16／展开节点 100000、单数组 4096 项；表达式长度 512B、128 token、深度 16、求值步骤 256，并检查算术溢出。表达式仅提供受限运算及 `bit()`、`len()`，不执行任意脚本。DTD、自定义实体、CDATA、处理指令拒绝。未实现的 DSL 扩展不应宣称支持，例如通用脚本、额外 checksum 算法及任意顶层 enumRef 扩展。

对外接口位于 [protocol/mod.rs](../musp-studio/src-tauri/src/protocol/mod.rs)：

```rust
compile_xml(xml: &str) -> Result<CompiledProtocol, ProtocolError>
CompiledProtocol::encode(code: u8, values: &serde_json::Value)
CompiledProtocol::decode(bytes: &[u8])
DecodedPacket::field(path: &str)
DecodedPacket::secret_spans()
DecodedPacket::redacted_bytes(bytes: &[u8])
```

核心解码对象为了协议处理保留实际字段值；应用层负责向 IPC、日志和运行记录输出脱敏视图。不能把底层 `decode()` 直接当作可公开日志对象。

## 3. 安全与真实 UDP：14 项应用回归

完整断言见 [application_security.rs](../musp-studio/src-tauri/tests/application_security.rs)。

| 测试 | 验证结果 |
| --- | --- |
| `project_roundtrip_preserves_draft_and_each_immutable_protocol_source` | 导出／导入保留草稿及每个发布版的独立 XML；导入时先恢复发布记录、最后恢复原草稿，避免发布操作覆盖草稿 |
| `project_import_rejects_checksum_tampering_before_creating_any_project` | checksum 不符拒绝，未留下新项目 |
| `checksums_do_not_replace_validation_of_published_xml` | 即使重新计算 ZIP checksum，带非法 XML 的发布版仍拒绝 |
| `duplicate_protocol_version_records_are_rejected_before_import` | 重复协议／版本记录在创建项目前拒绝 |
| `crc_failures_mask_header_and_auth_issue_tokens_before_persistence` | 真实双 UDP 端点，共 9 个坏包／未知包产生 18 条 TX/RX；验证 CRC 损坏、Token 长度缩短、magic 损坏、auth flag 清除、AUTH_ISSUE 信令码损坏均不在日志 JSON 留下已知 Token；普通未知 0xFE 报文保持可查看的 HEX；没有错误业务更新 |
| `transaction_remote_and_session_mismatch_cannot_apply_configuration` | 错误事务号、来源地址或会话号不能更新设备；正确 ACK 才更新；重复 ACK 不重复创建 |
| `queued_packets_keep_their_original_project_after_endpoint_stop_and_switch` | 停止端点、切换项目后，队列中的旧报文仍归原项目，不污染新项目日志 |
| `delayed_peer_response_does_not_escape_through_a_restarted_endpoint` | 对端延迟 400ms 回复期间停止／重启同 ID 端点，700ms 观察窗口内未从新代次发出旧回复 |
| `session_token_alias_is_redacted_from_snapshots_and_finished_run_storage` | 实际完成流程中，将 session_token 复制到通用变量名 credential，快照及完成记录仍不泄露值 |
| `virtual_peer_resources_are_isolated_by_remote_controller` | 不同真实 remote 即使逻辑源地址及资源标识相同，也不能释放另一个控制端的资源；不存在的释放返回 01 |
| `session_token_requires_matching_endpoint_protocol_and_remote` | 合法作用域可取 Token；分别更改端点、协议发布版、remote 均拒绝 |
| `auth_issue_routes_to_the_registered_session_in_a_second_system` | 实际 UDP 的 AUTH_ISSUE 归入第二个系统的已注册会话，不依赖 first-system 假设 |
| `virtual_peer_registration_sessions_are_isolated_by_remote_controller` | remote A/B 使用相同逻辑源地址注册后，A 的后续注册仍使用自己的会话 |
| `closed_remote_udp_port_does_not_disable_the_receiving_endpoint` | Windows 上向关闭的本机 UDP 端口发送后，端点仍可接收独立 socket 的后续包，并可正常停止 |

[udp_transport.rs](../musp-studio/src-tauri/tests/udp_transport.rs) 另有两项直接网络回归：真实双 socket 往返／停止释放端口／绑定占用错误，以及队列满时丢弃计数可见／包长有界。这些测试不使用 UI mock 替代 UDP。

最后两类缺陷有先失败、再修复通过的实证：

- Windows UDP 的可恢复 `ConnectionReset`／`ConnectionRefused`／`Interrupted` 曾结束接收循环。修复后累计 `socketErrors` 并继续接收，取消分支优先退出；小回归约 0.16 秒通过。
- 坏包脱敏曾依赖未损坏的 magic、auth flag 或信令码，导致特定损坏包日志出现 DE AD BE EF／CA FE BA BE。修复使用端点绑定的 MUSP schema 辅助判断；疑似认证扩展从秘密起点到帧尾遮蔽，AUTH_ISSUE 同时识别受损信令码但仍符合其长度结构的包。新增案例通过。

坏包策略只对可识别的 MUSP 认证区作保守遮蔽，普通 UNKNOWN 报文保留调试价值；没有把有限案例扩大为“任意损坏二进制都能可靠定位所有秘密”的保证。

## 4. 并发、会话与恢复审查

审查发现并由相应分工修复的边界包括：旧项目队列报文错误归属、旧代次延迟回复从重启端点发出、Token 复制到别名后泄漏、跨端点／协议版／remote 取用 Token、第二系统认证路由错误，以及虚拟对端在不同 remote 间共享注册／资源标识。上节测试给出了这些边界的实际覆盖。

相关实现采用报文的 projectId、generation 和取消令牌，原项目落库与当前项目业务路由分离；发送旧回复时校验端点代次。运行记录收集已知秘密值并递归脱敏别名；Token 获取绑定端点、协议发布版及 remote。这里描述的是已检查的实现和对应回归，不等同于所有并发交错的形式化证明。

本次最终局部只读审查未新增可复现的 P0/P1。全量桌面交互、安装、卸载和外部设备互通属于主交付验证范围，本报告不据后端回归宣称其完成。

## 5. SQLite 存储：两项兼容性回归

见 [persistence_storage.rs](../musp-studio/src-tauri/tests/persistence_storage.rs)。

- `compressed_packet_storage_preserves_query_export_and_legacy_text_rows`：真实解码树写入带 `MSPZ1` 标识的 zlib BLOB，样本压缩体积小于原 JSON 的三分之一；查询及 JSON 导出还原原对象；直接插入旧 TEXT 格式、重开数据库后查询及 JSONL 导出仍一致。
- `damaged_compressed_packet_is_reported_without_returning_fabricated_decoded_data`：损坏的压缩 BLOB 查询及导出均报错，不返回伪造成功解析结果。

压缩仅影响存储格式，未丢弃字段，也不改变实际解析展示。实现先将 JSON 序列化为完整缓冲区再压缩，压缩在数据库锁外完成；读取兼容旧 TEXT。SQLite 使用 NORMAL synchronous、缓存 prepared statement 和范围删除控制保留数量；没有以未持久化的后台批量缓存代替成功落库。历史压力样本里保留行数可略超过 50000，因为清理按批次触发，保留量不等于累计处理量。

## 6. 新增 settings／deviceEvents 导入导出：只读检查

本轮检查 [project_io.rs](../musp-studio/src-tauri/src/project_io.rs)、[archive.rs](../musp-studio/src-tauri/src/archive.rs)、[persistence/mod.rs](../musp-studio/src-tauri/src/persistence/mod.rs) 及 `device_history` 查询，未修改生产代码。

观察到的范围约束：

- 导出 settings 和 device_events 的查询都带当前 project ID；导入创建新 project ID，所有新增实体均写入该 ID。`device_history` 也先按项目取实体，再按 deviceId 过滤。
- 老 ZIP 缺少这两项时按空数组兼容。设置中的 heartbeatTimeoutMs 必须为 0，或 1000..3600000；事件必须有非空 id 和 deviceId。校验在创建新项目前执行。
- 正常产生的设备历史只记录 name、systemId、state、configVersion、registrationValid、identities、activeCalls 的前后摘要，不记录完整配置或 Token；项目 JSON 导出经过递归敏感键脱敏。
- ZIP 条目在内存中读取，不按归档提供的路径解压到磁盘；拒绝绝对／盘符／反斜杠／`.`／`..` 路径及重复条目。限制压缩文件 20MB、2048 个文件、单条目 2MB、展开合计 50MB，并校验清单与 SHA-256。
- 导入对发布版 XML 再编译，检查版本唯一性、连续性和流程引用；失败后清理新项目。checksum 用于完整性检查，不是可信来源签名。

这部分没有新增的独立 settings／deviceEvents 往返测试，不能列作实测通过。事件目前只做必要 ID 校验，不是完整历史事件结构校验；没有发现其可跨项目写入或覆盖已有项目的实证 P1。导入 domain 时正常保存逻辑可能另外产生当前导入状态的历史摘要，不应将历史数量的完全相同视为已验证行为。

## 7. 保留的性能证据与用户取消记录

测试工具为 [examples/udp_load.rs](../musp-studio/src-tauri/examples/udp_load.rs)，不在普通 `cargo test` 中运行。它从独立 socket 发送真实本机 UDP，经过 Studio 的接收队列、decode 和 SQLite；使用 30B 合法 0x32 报文、release、headless。它未测 UI 渲染、完整 512B 负载或外部网络，`kernelRx` 指接收 socket 已读到的报文数，不能代替业务成功数。

环境证据：[udp-load-hardware.json](../musp-studio/test-results/udp-load-hardware.json)：Intel Core 5 220H，12 核／16 线程，约 15.55GiB 内存，Windows 10 专业工作站版 19045；不是需求示例中的 Windows 11。取消轮次使用的二进制摘要保留于 [udp-load-binary.json](../musp-studio/test-results/udp-load-binary.json)。

已完成的旧版长测在未压缩日志实现上执行，期间存在其他构建活动，不能代表最终版本的独占性能：[原始 results.json](../musp-studio/test-results/udp-load-1792031287686/results.json)。

| 目标速率／发送时长 | 实际发送 | socket 已接收 | 成功处理并落库 | queueDropped | processingErrors／socketErrors | 排空时间 | SQLite 保留行数 |
| --- | ---: | ---: | ---: | ---: | --- | ---: | ---: |
| 1000/s，300 秒 | 300000 | 300000 | 270415 | 29585 | 0／0 | 1.023 秒 | 50415 |
| 5000/s，60 秒 | 300000 | 300000 | 93213 | 206787 | 0／0 | 1.965 秒 | 50628 |

该结果明确存在应用队列丢包，**没有达到上述速率下全部处理落库的目标**。短测保留作定位资料，不外推为长期保证：

| 诊断轮次 | 1000/s × 3 秒：落库／丢弃 | 5000/s × 2 秒：落库／丢弃 | 原始证据 |
| --- | --- | --- | --- |
| 初始存储 | 3000／0 | 4738／5262 | [results.json](../musp-studio/test-results/udp-load-1792030950223/results.json) |
| SQLite 配置优化后 | 3000／0 | 9417／583 | [results.json](../musp-studio/test-results/udp-load-1792031239705/results.json) |
| 初始压缩实现 | 3000／0 | 5737／4263 | [results.json](../musp-studio/test-results/udp-load-1792031827144/results.json) |
| 完整缓冲区压缩后 | 3000／0 | 7548／2452 | [results.json](../musp-studio/test-results/udp-load-1792031936333/results.json) |

后续长测在用户提出“不需要这么大的压测，不会有这么快的包”后立即停止。保留的 [cancelled.json](../musp-studio/test-results/udp-load-1792032082387/cancelled.json) 明确标为 `cancelled_by_user`、`metricsIncomplete=true`：最后一次 150 秒采样为发送 150005、接收 150004、队列丢弃 5828、工作集 28045312B。这是停止前采样，不是最终累计数据；该轮 5000/s 阶段没有执行，没有生成完成结果。数据库／WAL 样本保留。

高频压测不再作为交付阻塞项，也不继续据此扩大架构改造。本报告保留不达标与取消记录，交付说明不得将其改写成高吞吐达标或零丢包承诺。
