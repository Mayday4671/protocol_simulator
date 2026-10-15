# MUSP Studio 测试与交付记录

日期：2026-10-15。原始需求、业务基线及七张原型图保持原文；本文件记录实际实现和实测证据。

## 环境

Windows 10 x64 build 19045；Intel Core 5 220H（12核/16线程）、15.55 GiB RAM；Node 22.16.0、pnpm 10.24.0、Rust 1.97.1 MSVC、VS Build Tools 2022、WebView2 151.0.4129.101。本机已有WebView2，不替代全新离线虚拟机验收。

工程位于 `musp-studio`；原始输出及截图位于其 `test-results/`。

## S0–S6阶段记录

| 阶段 | 实现及验证 | 状态 |
|---|---|---|
| S0 | Tauri/React工程、MSVC原生EXE、SQLite迁移、项目/系统、主题及七页；按用户反馈合并标题栏，真实窗口控件通过。 | 核心通过 |
| S1 | Rust XML DSL/AST、共享结构、条件、数组、Bit、表达式、CRC；99B/31B向量往返；草稿/校验/不可变发布；字段多选复制、拖动、跨信令粘贴及引用检查。 | 核心通过 |
| S2 | Tokio真实多端点UDP、停止释放/占用反馈/离线ICMP恢复；坏包保留；SQLite压缩存储兼容旧TEXT；分页/方向过滤/HEX双向定位/CSV、JSON、JSONL。 | 核心通过 |
| S3 | 手工/报文创建设备、逐条AA、两种配置映射、注册/认证会话；严格三态；脱敏会话及持久变更历史；100设备/2控制端关联测试。 | 核心通过 |
| S4 | React Flow与Rust解释器；发布快照；发送前登记等待；完整关联、超时重试、重复/迟到、取消、分支、循环、并行、子流程、暂停/继续/单步。 | 核心通过 |
| S5 | 视频四项/语音两项RX/TX实际确认；带宽分列；主动结束/取消/离线与独立清理；UNKNOWN不回池。 | 核心通过 |
| S6 | 七个可编辑流程模板、故障注入、项目ZIP、说明书、许可证记录；真实WebView2 E2E；内嵌WebView2的NSIS安装包；安装、自测、卸载数据保留。 | 本机验收通过 |

核心通过表示表内已覆盖能力，不等于所有增强界面均已实现，边界见文末。

## 测试证据

- 原始 `generate_golden_vectors.py`：退出0；CONFIG_SET 99B/CRC D35D，CONFIG_RESULT 31B/CRC FECB。
- `cargo test --manifest-path src-tauri/Cargo.toml --locked -j 2`：最终106项通过、0失败，含认证、主动结束/取消、心跳历史、越界及未发布草稿拒绝。原始输出 `test-results/cargo-test-final.txt`。首次默认并行编译触发Windows页面文件不足，保留失败日志，改为2个编译任务后成功，未修改系统设置。
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check` 与 `cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets -j 2 -- -D warnings`：最终退出0。
- `pnpm test`：最终12文件36项通过；字段复制/引用、Bit、HEX、协议Tab、多选/拖动、标题栏API、草稿运行保护、跨项目查询隔离和通话类型保存等。
- `pnpm build`：TypeScript/Vite成功，Monaco体积警告保留；编辑器本地加载，无CDN。
- `scripts/desktop_e2e.py`：真实Tauri WebView2操作11项通过，5台设备实际UDP配置注册；45条项目报文含1条刻意损坏报文；七页、主题持久化、XML校验、HEX双向选择、方向过滤、导出导入，无未处理JavaScript异常。`test-results/desktop-dev-e2e/report.json`。
- `scripts/window_e2e.py`：真实双击最大化/还原，项目下拉控件不触发拖动。`test-results/window-e2e.json`。
- 100设备/2控制端：峰值100等待；最终100 ONLINE/100独立会话，错来源/错事务/无等待响应未污染会话。
- 安全：DTD/ENTITY拒绝，表达式白名单，上限检查，ZIP路径穿越/校验和，发布版不可变，跨项目迟到包隔离，端点重启隔离，Token在坏magic/flag/code/CRC下脱敏；普通UNKNOWN保留HEX。详见 `REVIEW.md`。
- 设备历史/业务校验：资源ID/版本越界拒绝、设备变更持久化不存Token、项目备份保存设置与历史已通过。

## 用户调整的验收范围

用户明确表示“不需要做这么大的压测，我不会有这么快的包”。已停止1000/5000包每秒持续压力测试，不再将该吞吐指标作为本次交付门槛。保留已执行失败/中止记录，**不宣称达到原设高吞吐目标**。接收、处理、失败、队列丢弃显示真实计数；保留普通回环、超时/重试和关联测试。

## 安装记录（首次交付）

最终 `pnpm package:windows` 成功，退出0：`release/MUSP-Studio_0.1.0_x64-setup.exe`，224,126,597字节（213.74 MiB）。内嵌微软签名Valid的WebView2离线安装器。首次交付 SHA256：`B26284747E9517ACFE3F17B86CC80239D31157A5FD051E1D14136BC21D06320C`；更新后的安装包见下节。

- 首次安装和最终升级安装均退出0。安装后二进制与构建产物仅相差Tauri官方打包标记 `__TAURI_BUNDLE_TYPE_VAR_UNK` → `...NSS` 的3字节，逐字节核验无其他差异。证据 `install-final.json`。
- 最终安装EXE `--self-test`退出0：SQLite/XML/双端点真实UDP/CRC/唯一匹配/0x22提交/4条报文落库全过，队列丢弃与socket错误均0。证据 `installed-self-test-final.json`。
- 最终安装版 `http://tauri.localhost/` 的12项桌面检查通过，无开发服务器依赖：包含可视化画布编辑→保存草稿禁止运行→发布→按钮启动实际UDP流程，以及配置后仍保留设备ONLINE。项目49条报文，1条刻意损坏报文用于验证诊断；未处理脚本异常0。证据 `desktop-installed-final/report.json`。
- 自定义标题栏最小化、最大化/还原、双击与项目控件隔离通过；点击真实关闭按钮后应用退出，仍在监听的UDP端口可立即重新绑定。证据 `window-e2e.json`、`close-window-e2e.json`。
- 最终卸载退出0，应用EXE删除，项目SQLite保留且卸载前后SHA256一致。证据 `uninstall-final.json`。
- 验收后重新安装到当前用户 `%LOCALAPPDATA%\\MUSP Studio`，提供正常快捷方式与可直接运行的EXE。证据 `installed-location.json`。

所有安装、自测、卸载结果汇总在 `test-results/installation-final.json`；发行目录附离线HTML验收报告及七页真实安装版截图。

## 明确边界

- 没有干净Windows 11虚拟机、无WebView2环境或外部MUSP硬件联调证据。
- 高级字段通过XML/节点JSON配置；未为每个字段提供专用表单。
- 已有多协议草稿Tab；统一协议/设备/流程顶层Tab、模板分类收藏库尚未实现。
- Token不保存为可恢复会话；重启需重新注册，旧运行不自动重发。未确认资源恢复UNKNOWN，须真实确认或带原因人工核实。
- 应用未配置代码签名；SHA256用于完整性核验。
- 通用8B剩余字节、候选Bit等业务歧义见 `OPEN_QUESTIONS.md`。
## 页面文案更新与协议演示录入

2026-10-15：删除七个业务页及系统设置的标题副文案、页脚口号、装饰标签和重复介绍，保留字段单位、业务前提及错误提示。现有前端测试36项通过，生产构建和NSIS打包成功；Rust业务代码未改动，本轮未重复全量Rust测试或卸载测试。

- 新安装包升级退出0；大小224,128,852字节。SHA256：`6E260B2350BF5F72CAA23216CE0280B42B2455B85BFD144CC1E8E433C126B675`。
- 安装后二进制核验通过；7项独立自测通过，真实UDP收发各2条、无丢包或socket错误。证据 `test-results/copy-cleanup-install.json`。
- 已安装版逐页检查8页，标题副文案均已移除，未处理脚本异常0；检查HEX与字段联动、Bit展示、AA/170/配置成功。
- 本机项目“MUSP v0.8 信令交互演示”导入并发布34条MUSP v0.8信令。首次执行五设备配置、5轮注册和3轮心跳，保存56条收发记录；升级后重新注册并运行3轮心跳，合计108条历史记录，非压力测试。
- 初次56条记录的CRC与长度独立重算通过，14组事务匹配；配置逐条AA后提交，5台设备完成注册。协议候选Bit及8B结构仍保留候选标记。
- XML、项目ZIP、CSV/JSON报文及截图保存在 `test-results/live-demo/`。对端是明确标注的本机虚拟UDP对端，未连接外部设备。
