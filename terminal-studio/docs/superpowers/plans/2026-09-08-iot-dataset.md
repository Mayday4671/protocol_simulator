# IoT 演示数据替换计划

> 执行方式：在当前会话按 executing-plans 执行，先验证后替换。当前目录无 Git，不创建仓库或提交。

**Goal:** 将应用中的 13 条旧测试协议、8 张旧测试字典替换为可实际操作的三套复杂物联网协议和完整字典。

**Architecture:** 可重复生成的 Python 数据定义输出标准 simterm-import XML；先用独立 SQLite 数据库和真实 Rust 引擎验收，再对明确核对过的正式数据库做在线备份和单事务替换。保留通道地址、日志；若发现新协议、终端资料或心跳绑定则中止替换重新核对。

**Tech Stack:** Python 标准库（SQLite / ElementTree / unittest）、现有 Rust 协议引擎、Playwright。

## 数据设计

- 环境采集、门禁安防、边缘网关三个合成协议族；各有接入管理、参数配置、业务上报三个分组，共 9 组。
- 每族 18 条协议，共 54 条：注册、注销、心跳请求与应答；配置读写和查询请求与应答；实时/告警/批量业务及应答。
- 公共头包括族标识、版本、消息码、源/目的标识、序号、Session；每条协议的族标识+消息码唯一匹配，避免跨族串协议。
- 配置、业务消息有三路条件分支；主要复杂报文同时有一组分支列表和一组公共扩展列表。默认数量为 03/04/08，可改为 00；单位记录有 6—8 个可编辑字段、不同字节宽度和字典绑定。
- 全部用户录入值用十六进制。每组至少 12 张字典，常用枚举有实际中文含义，测点/设备/错误码等字典有 32—96 项，覆盖字典分页和条目分页。
- 不将尚不支持的 Bit 声明和递归变长记录伪装成可用样例。原能力探针作为源码测试资源保留，不导入应用。

## 执行清单

- [x] 新增 `scripts/test_iot_dataset.py`：先断言输出存在、54 条协议/9 分组/每组至少 12 字典、全 Hex、分支默认值、字典引用、列表长度、备份/事务及旧数据替换边界；运行获得预期失败。
- [x] 新增 `scripts/iot_seed_data.py` 数据定义与 `scripts/iot_dataset.py` 安全导入：生成 `protocols/IOT_LAB_2.0.xml`；将 XML 映射为原有数据库 schema，先只安装到测试副本。实际 130 张字典、3266 条条目。
- [x] 新增 `src-tauri/src/bin/e2e_iot_dataset.rs`（仅 e2e 特性）：对新数据全量校验、默认值发包、三分支/零列表、唯一匹配、真实本机 UDP 注册应答与跨族 Session 隔离。所有网络地址限定 loopback。最终 699 帧、33 对真实自动应答、270 个畸形/截断报文不匹配测试通过。
- [x] 用新数据库数据驱动实际前端页面，核对分支、列表默认值和两层字典分页，而不是另造简化 mock 协议。Python Playwright 不可用，复用项目 Node Playwright 环境；仅替换 native IPC，协议和字典均读取真实导入数据。
- [x] 对正式数据库先执行 SQLite backup，核对旧协议 ID 集合、8 张字典、空 terminal_profiles/heartbeat_bindings；单事务替换协议/字典/系统/分组，保留节点地址和日志，更新节点到新分组的绑定；任何检查失败回滚。
- [x] 验证正式数据库 counts/foreign_key_check/integrity_check；将新 XML 放入 `dist_portable`。数据由现有 EXE 读取，不需改动或重新构建桌面程序。

## 验收命令

```powershell
python scripts/test_iot_dataset.py
python scripts/iot_dataset.py generate
cargo run --features e2e --bin e2e_iot_dataset -- E:\My_Project\Simulated_Terminal\artifacts\iot-dataset-validation.db
node scripts/ui_iot_dataset.cjs
python scripts/iot_dataset.py install --db C:\Users\KYD-PC-357\AppData\Roaming\com.simterm.desktop\simterm.db --replace-legacy --backup-dir E:\My_Project\Simulated_Terminal\backups
```

首次执行真实替换之前，所有独立数据校验必须通过。正式替换脚本不停止用户进程、不修改网络地址、不发送报文、不自动清空新建数据。

## 独立审查后的补充

- 替换前固定已核对的旧协议/字典/分组内容 SHA256；即使 ID 和条数没变，只修改某个字段或条目含义也中止。事务内再次核对，同时检查备份期间的配置和终端状态并发变化。
- 应答不声称固定的接收条目数；改为 `确认消息码 = {req:消息码}`，真实 UDP 测试核对回执对应原请求。
- 旧测试数据备份可恢复；源码里被核心单元测试引用的历史 XML 保留，但不导入本次正式数据。

## 交付验证

- 2026-09-08 16:23 正式数据库替换完成：3 个系统 / 9 个分组 / 54 条协议 / 130 张字典 / 3266 个条目。
- 原数据库在线备份：`backups/simterm-before-iot2-20260908-162355-708615.db`。
- 2 个原通道除更新新分组绑定外所有字段不变，681 条历史日志保留；正式库完整性和外键检查通过。
- Python 数据与安全安装测试 7 项通过；核心 Rust 测试 75 项通过；真实 UDP 最终测试通过；三族页面分支/子条件/默认数量/空列表/折叠状态及两层字典分页验收通过。
- XML 交付：`dist_portable/IOT_LAB_2.0.xml`，与源文件 SHA256 一致。当前机器无需重复导入，重新打开程序以清除旧页面缓存。
