# MUSP Studio 测试与交付记录

日期：2026-10-15。原始需求、业务基线及七张原型图保持原文；本文件记录实际实现和实测证据。

## 环境

Windows 10 x64 build 19045；Intel Core 5 220H（12核/16线程）、15.55 GiB RAM；Node 22.16.0、pnpm 10.24.0、Rust 1.97.1 MSVC、VS Build Tools 2022、WebView2 151.0.4129.101。本机已有WebView2，不替代全新离线虚拟机验收。

工程位于 `musp-studio`；原始输出及截图位于其 `test-results/`。

## 当前交付：0.1.2 导入修复后的安装版

0.1.2 首次候选包的项目导入阻断已修复，已重新构建 NSIS、安装、自测并从客户端复测。本节记录截至截图 **215** 的实际结果：**117–188 对应修复前候选包，189 起对应修复后安装包**；两者虽然同为 0.1.2，二进制散列不同，验收范围分别列出。01–116 保留为旧版及修复过程记录。代码回归、安装自测和客户端操作分别统计，不互相替代，也不将前一候选包全部步骤声称为最新二进制重新执行。

### 代码回归与安装

| 验证层 | 实际结果 | 原始证据（相对 `musp-studio/`） |
|---|---|---|
| Rust 有界回归 | 169 项通过、0 失败；按用户范围跳过 1 项百设备注册用例，不进行高频压力测试 | `test-results/uav-final-rust-test.txt` |
| 前端有界回归 | `pnpm test --maxWorkers=2`：22 文件、102 项通过 | `test-results/uav-final-frontend-test-bounded.txt` |
| 导出修复后的定向回归 | Rust 24 项通过，含新增 5 项；Workspace 前端 8 项通过，含新增 1 项；TypeScript 检查通过。与上面完整回归分别统计，不能相加为不重复用例总数 | 本次修复的定向测试输出 |
| 格式、静态检查及生产构建 | Rust fmt、全目标 Clippy、TypeScript/Vite 构建通过；Vite 大包提示保留 | `test-results/uav-final-fmt.txt`、`uav-final-clippy.txt`、`uav-final-build.txt` |
| 导出修复后的格式与静态检查 | fmt、全目标 Clippy 通过 | `test-results/uav-export-fix-fmt.txt`、`uav-export-fix-clippy.txt` |
| 修复后 NSIS 构建与隔离安装 | 0.1.2 重新构建成功；实际安装退出 0，原安装二进制未改动，应用注册信息已恢复 | `test-results/uav-final-package-r2.txt`、`test-results/flow-workspace-install/report.json` |
| 修复后安装 EXE 自测 | 7 项通过：SQLite 迁移、内置 XML 编译、双端点真实 UDP、编码与 CRC、唯一匹配、0x22 业务提交、报文落库；TX/RX 各 2 条，队列丢弃和 socket 错误均 0 | `test-results/uav-installed-selftest/self-test.json`，记录内目录为 `uav-installed-selftest-r2` |

首次前端默认并发运行中，Calls 的一条断言超过 5 秒；保留 `uav-final-frontend-test.txt`，降低并发后完整重跑通过，没有修改业务断言或放宽超时。Rust 本轮还修复了取消时将尚未下发的 Reserved 当作远端资源释放的问题：本地预留仅归还一次，已发配置仍等待真实释放确认；这是代码回归结论，不冒充最终版客户端取消场景实测。

当前交付安装包为 `release/MUSP-Studio_0.1.2_x64-setup.exe`，已核对 SHA256：`910605FCC1DFA6930352BF9D66DCC1E8D90EF6B17529C3386F693DA01A19AC94`；已安装 EXE SHA256 为 `73D8CA815108E08F94D65373731B7CE6B95140EC5B240D3A7205586ACB6BDC86`。本机已有 WebView2，安装通过不代表全新离线虚拟机已经验证。

修复前候选包 SHA256 为 `6FF3F824A0292ADB5C6936F03C8B2D7A296DDBF32A158F969D95FC7064E5785B`，其 EXE 为 `7F41B32A6A584797766A63ECD97E597A24EA46CBB3D20A6EE3069DA8BA9413EC`。失败记录保留在 `test-results/uav-installed-client/candidate-1.json`，`passed=false`；旧安装记录为 `test-results/uav-first-candidate-install.json`，不与当前安装记录混用。

### 修复前候选包的客户端实测（117–188，历史保留）

从已安装 0.1.2 的 `http://tauri.localhost/` 新建“无人机 UDP 联调示例”，通过界面完成协议选择、模板创建、字段填写、流程运行和结果查看。控制端为 `127.0.0.1:9001`，明确标注的本机虚拟 UDP 对端为 `127.0.0.1:9002`。操作不调用应用业务 IPC、不直接写数据库、不从后台填充项目。动作记录为 `test-results/uav-client-audit/actions.jsonl`；下表编号对应同目录 PNG 截图。

| 用例 | 已观察到的成功判定或预期拒绝 | 截图 |
|---|---|---|
| 从空项目开始、五设备配置 | 通过客户端创建项目、选择协议和模板，21→22 完成五设备配置 | 117–123 |
| A/B 免认证注册、三轮心跳 | 选择实际设备运行注册，A/B 注册成功；三轮 31→32 完成 | 124–127 |
| 视频本机就绪启动 | LOCAL_READY 完成 A/B 四项 RX/TX 资源，当前运行 13 条真实收发；通话进入通信中 | 128–135 |
| 并行语音统一通知启动 | CONTROL_NOTIFY 建立两项 RX/TX 资源，视频与语音同时存在 | 136–140 |
| 独立结束和资源回收 | 控制侧结束视频后语音继续；随后 73 申请结束语音，全部确认后可用容量恢复 20000 kbps | 141–144 |
| C 认证注册 | 01→02→11→12→03→04 六条信令关联同一运行，认证材料在界面中遮蔽 | 145–148 |
| 已认证业务命令 | 51→52 非空参数和空参数均完成；对端业务拒绝时流程明确失败，显示实际 errorCode=4660 | 149–154 |
| 注册业务拒绝 | 收到拒绝 02 后当前运行仅 01/02 两条即失败，不继续确认注册或无意义重试 | 155–159 |
| 单设备五参数配置 | 模式 02 写入五组参数，已在线设备保持 ONLINE | 160–164 |
| 项目导出 | 从客户端成功导出 `release/无人机UDP联调示例.zip` | 165 |
| 容量不足 | 容量 64 kbps，拟分配 RX64+TX64 共 128 kbps；仅 65→66，resultCode=04，无 70/71 下发，运行给出容量不足原因 | 166–168 |
| 部分配置失败及保守回收 | TX 配置被拒绝后实际补偿；RX 确认释放、归还 64，TX 的 77=01 保持 UNKNOWN 并占用 64；大池可用 19936，小池仍为 64。当前运行六条仅到 72，补偿报文需在当前端点查看 | 170–172 |
| 导出后重新导入并启动监听 | 当时未通过：两个监听报 `invalid type: string "[已脱敏]", expected u32`。`peerPolicy.tokenValidSeconds` 被过宽的敏感字段过滤替换；本缺陷已在重打包后复测修复，见下表 | 177–179 |
| 实际 XML 文件导入并发布 | 文件上传后生成 34 信令草稿并发布 v1；181/182 的 helper 选择器未命中由 AntD loading 残留导致，不计业务失败 | 183 |
| 有依据的人工核销 | TX72=01 未分配、77=01 不存在，且已停止本机模拟端；输入核实依据并勾选确认后核销 UNKNOWN64，池恢复 20000，再恢复 tx0 策略 | 184–186 |
| 错误 JSON 输入保护 | 错误 JSON 不造成白屏；取消后保留输入 | 187–188 |

### 修复后安装包的客户端复测（189–215）

| 客户端动作 | 实际结果 | 截图 |
|---|---|---|
| 重新打开原验收数据 | 仍选择原项目；监听保持停止，没有自动重发旧运行 | 189 |
| 重新导出项目 | 从 UI 重新生成 `release/无人机UDP联调示例.zip`，使用修复后的导出规则 | 190–192 |
| 全新数据目录导入并启动 | 在 `test-results/uav-final-installed-data` 从 UI 导入、切换项目，两个端点实际启动成功，原阻断已消除 | 193 |
| 导入项目中的 A/B 注册 | 两台设备分别完成 01→02→03→04，各 4 条真实收发 | 194–196 |
| 导入项目中的三轮心跳 | 3 轮 31→32 共 6 条，流程完成 | 197–199 |
| 导入项目中的 C 认证注册 | 01→02→11→12→03→04 完整 6 条，流程完成 | 200–201 |
| 导入项目中的认证业务命令 | 取当前认证会话的内存 Token，51 空参数→52 成功 | 202–203 |
| 两通道状态上报 | 监控表单选择 41 并填写两通道数组，实际 43 B TX/RX；温度 -1250、电平 -12/15、频偏 -30/20，HEX/DEC 及在线、启用、关闭中文含义正确 | 204–208 |
| 导入后被动语音建立 | 线上通话号 8200，RX/TX 各请求 128、分配 64；65→66、70/72、71/72、75/79 共 8 条完成，池可用 19872 | 209–210 |
| 控制侧结束该语音 | 在通话页面选中 8200，结束并确认；RX/TX 各 64 经实际确认回收，池恢复 20000，无 UNKNOWN | 211–213 |
| 关闭、重启及最终退出 | 两监听运行时关闭并重启，仍选择“无人机 UDP 联调示例（导入）”；5 台设备 OFFLINE、两端点停止、TX/RX 为 0，无自动重发，静态协议和设备保留；随后关闭窗口正常退出、不占端口，用户原安装未改 | 214–215 |

上述新包复测均通过客户端界面执行，没有调用 UI 业务 IPC 或直接写数据库。旧 ZIP 中已经被替换或丢失的认证运行时引用不会被猜测恢复，必须从原项目用新版重新导出；不能把旧坏 ZIP 当作本次已验收示例。修复前原文件保留为 `test-results/uav-original-export-before-fix.zip`。

操作期间未观察到未处理 JavaScript 异常；两处 pageerror 记录文件不存在，该陈述是观察结果，不把缺少日志文件当作独立的异常监测证明。

### 本次仍未完成或不在结论内

- 新 ZIP 的导入、两个监听启动、注册、心跳、认证业务命令、状态数组、语音建立及释放、重启和正常关闭已在新包完成；其他候选包场景不自动计为新包重测。
- 部分配置失败的资源结论已观察到；补偿是域事件，目前仅在“当前端点”看到，未完整归属“本次运行”，记为 UX 待优化，不声称本次运行列表已包含完整清理链路。
- 旧版 01–116 的部分释放失败、取消、离线、坏 CRC、逐条非 AA 等记录保留为问题发现和历史回归；未在本节声称最终安装版已逐项重复。
- 完整 XML 定义了 34 条信令，不等于本次安装候选版已覆盖全部 34 条及其所有结果分支。客户端范围仅限上表。
- 未连接外部无人机或控制器硬件，未验证干净无 WebView2 的离线虚拟机；未配置代码签名，不宣称高吞吐指标。
- 旧项目的已发布 XML/流程快照不自动改写；新增中文枚举和 51 参数长度计算需显式选择新版协议及新建模板。

详细操作记录见 `musp-studio/docs/plans/2026-10-15-uav-client-acceptance.md`。以下各节保留此前交付的原始结论与版本，不代表 0.1.2 重复执行。

## 0.1.0 历史 S0–S6 阶段记录

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

## 0.1.0 历史测试证据

- 原始 `generate_golden_vectors.py`：退出0；CONFIG_SET 99B/CRC D35D，CONFIG_RESULT 31B/CRC FECB。
- `cargo test --manifest-path src-tauri/Cargo.toml --locked -j 2`：该版最终106项通过、0失败，含认证、主动结束/取消、心跳历史、越界及未发布草稿拒绝。原始输出 `test-results/cargo-test-final.txt`。首次默认并行编译触发Windows页面文件不足，保留失败日志，改为2个编译任务后成功，未修改系统设置。
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check` 与 `cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets -j 2 -- -D warnings`：最终退出0。
- `pnpm test`：该版最终12文件36项通过；字段复制/引用、Bit、HEX、协议Tab、多选/拖动、标题栏API、草稿运行保护、跨项目查询隔离和通话类型保存等。
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

## 0.1.0 交付时记录的边界

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

## 0.1.1 流程联调工作区

2026-10-15：根据“发送时看不到实际信令、需要切页、启动条件不清楚”的反馈，将发送配置、编码预览、运行控制和真实收发解析集中在流程页。

- 启动栏常驻，显示具体不可运行原因，唯一匹配的已运行端点自动选择；协议和信令从发布 XML 读取。普通字段可直接填值，数组、复杂结构和高级规则仍保留 JSON 编辑。
- 预览使用实际 Rust 编码器，展示 HEX、DEC、Bit 和中文解析；不创建运行、等待或发送，不替代实际收发记录。缺少变量明确列出，自动事务占位明确标记。
- 实际 TX/RX 记录运行实例和节点；本次运行精确过滤，当前端点范围保留未匹配和坏包。旧 SQLite 日志迁移后关联为空，不伪造所属实例。双解析面板的字节定位相互隔离，选中字段直接显示 HEX、DEC 和含义。
- 通用 UDP XML 不触发 MUSP 同码业务；MUSP 业务仅对明确的协议 ID 启用。缺失的正文 transactionId 不会被自动补值；显式事务、HEX 事务和嵌套头事务均按实际配置匹配，被动等待不套用 MUSP 会话条件。
- 保存至发布完成期间锁定编辑和流程切换，避免显示值与发布值不一致；停止运行和查看报文仍可操作。

本轮最终回归：

- `pnpm test`：16 文件、52 项通过。证据 `test-results/flow-workspace-frontend-test.txt`。
- `cargo test --locked -j 2 -- --skip one_hundred_devices_register_concurrently_with_two_isolated_controllers`（指定 src-tauri 清单）：133 项通过、0 失败，跳过 1 项百设备用例。证据 `test-results/flow-workspace-cargo-test.txt`。本轮不运行持续高频压力测试。
- `cargo fmt --all -- --check`、`cargo clippy --locked --all-targets -j 2 -- -D warnings`：退出 0。证据 `test-results/flow-workspace-clippy.txt`。
- 新增真实 UDP 回归覆盖主动发送、纯被动等待、并行运行关联、旧库迁移、同码通用协议隔离、显式事务条件及 HEX/嵌套头部事务；预览回归覆盖无副作用、缺变量、动态数组、共享结构与大整数精度。

0.1.1 安装与桌面验收：

- `pnpm package:windows` 退出 0。NSIS 安装包 `release/MUSP-Studio_0.1.1_x64-setup.exe` 为 224,175,050 字节，SHA256 `FCAE7D347725552BD157AA10CFAE895E871232E0AA5D151671E23C5ED894AA00`。生产 TypeScript/Vite 构建通过，沿用内嵌 WebView2 的 Windows EXE 安装方式。
- 在工作区独立目录执行实际 NSIS 安装，退出 0。逐字节核验已安装 EXE 与构建产物仅相差官方 NSIS 打包标记；原 `D:\soft\MUSP Studio` 二进制散列不变，安装注册信息恢复原位置和版本。证据 `test-results/flow-workspace-install/report.json`。
- 已安装 0.1.1 EXE 执行隔离自测，7 项检查通过：实际 MUSP 配置请求/响应、CRC、唯一匹配、设备提交和 SQLite 落库，共 4 条实际记录，socket 错误和队列丢弃均为 0。证据 `test-results/flow-workspace-installed-selftest/self-test.json`。
- 关闭开发服务器后，从已安装程序的 `http://tauri.localhost/` 执行 `scripts/flow_workspace_e2e.py`，7 组界面检查通过。通过页面按钮启动；独立 Python UDP socket 收到实际 `21 2B 81 02 07 09` 后返回 `22 2B 81 02 07 09`；被动接收 `73 19 81 02 07 09` 后按接收字段发出 `21 19 81 02 07 09`；第三个等待流程超时失败且没有伪造报文。三流程合计 4 条收发记录，未处理脚本异常 0。证据 `test-results/flow-workspace-installed/report.json`。
- 已检查 1440×1000、1280×800 的真实安装版截图，启动栏、实际收发区可见，无页面横向溢出。截图随发行报告保存为 `screenshots/flow-*.png`。

本次不重复 0.1.0 卸载和七页巡检，不将其历史结果冒充本版验收；也不替代外部硬件或全新无 WebView2 虚拟机验证。测试项目使用独立数据目录，未编辑用户日常项目。

## 2026-10-19 原版基座：配置条目数与实际数组联动

当前工程为 `terminal-studio`，未恢复上文已归档的旧引擎/UI。修复配置条目数01仍固定展示/发送五条的问题，配置0x21/0x22使用真实动态数组，自动生成长度及CRC；发送前校验条数，模式切换与缩减数量保留编辑草稿。

- 前端生产构建、2个前端回归脚本、97项Rust core测试通过；补充零条目边界单测通过。
- 最终EXE实际界面操作与本机UDP对端收发：0条49/26B、1条59/27B、5条99/31B；模式01/02、事务回填、隐藏草稿恢复、非法数量阻止发送均验证。
- 记录与当前边界见 `terminal-studio/docs/MUSP_v0.8_基座试录差异.md`；证据在 `terminal-studio/artifacts/base-uav-trial/dynamic-*`。
- 新版 `.uav-array-trial/` 使用独立备份库，原客户端未关闭。本轮未进行NSIS安装测试。

## 2026-10-19 原版基座：按协议定义展示报文分区

新增字段分区/分区名称编辑与 XML 往返保留，解析按实际顺序显示公共头、扩展头、净荷、尾部或未解析字节。支持多个头 A/B/A、不带头尾的协议及数组逐条分组，不以字段名或固定偏移猜测分区。修正头部字节数、收包方向、截断数组与缺失尾部的详情解析。

- 前端与 Windows release 构建通过；core 全量 107 项通过，最终截断边界修正后的分区专项 11 项通过；前端 helper/独立组件回归通过。
- 最终 EXE 从客户端录入两种对照协议，真实 UDP 验证组合头与三字节尾、纯净荷协议的发送/接收解析；无人机 0x21/0x22 实际 69B/28B 收发、两条数组及 AA 中文结果验证通过。
- 记录、截图、SHA256 见 `terminal-studio/artifacts/packet-layout/验收记录.md`；配置方式见 `terminal-studio/docs/报文分区.md`。
- 新版入口 `.packet-layout-trial/启动解析新版.cmd` 使用独立数据备份，原窗口未关闭。本轮未进行 NSIS 打包安装。

## 2026-10-19 解析分区配色与折叠

增加分区独立折叠、全部折叠/展开、导航自动展开和折叠摘要；加强各分区配色及深浅色对比。生产前端/EXE构建、组件交互回归、最终EXE真实69B/28B收发与折叠操作通过。未修改协议引擎，本轮不重复压力测试和NSIS安装。验收记录见 `terminal-studio/artifacts/packet-style/验收记录.md`，新版入口 `.packet-style-trial/启动解析新版.cmd`。

## 2026-10-19 解析字段卡片配色

字段卡片增加六色色条和浅底，数组同名字段同色，突出HEX并使用紧凑中文含义标签；保留多协议分区、折叠和完整DEC。前端/Windows EXE构建、组件回归及最终EXE真实69B/28B配置收发通过，实际深浅截图已检查。未修改协议引擎，未进行NSIS安装或压力测试。证据见 `terminal-studio/artifacts/packet-cards/验收记录.md`，入口 `.packet-cards-trial/启动解析新版.cmd`。

## 2026-10-19 协议编辑顺序与数组结构

修复非连续同条件字段被提前聚合的显示错序，数组结构独立展开并区分单条长度与默认数据总量。新整页回归先失败后通过；旧分支及动态数组回归、前端/Windows构建通过。最终EXE界面保存前后字段数据完全一致，真实UDP验证模式01一条59B/27B、模式02两条69B/28B。没有修改协议引擎或执行NSIS安装。记录见 `terminal-studio/artifacts/protocol-structure/验收记录.md`，入口 `.protocol-structure-trial/启动协议编辑新版.cmd`。

## 2026-10-19 分组多公共头与窗口标题栏整合

`terminal-studio` 支持分组内维护多套公共头，每条协议独立勾选并排列 A、B、A+B、B+A 或明确无头；旧头继承与同头重名字段兼容，失效或跨头歧义引用拒绝保存。Windows 白色原生标题栏并入标签栏，公共头编辑增加保存／放弃切组保护。

- Rust core 整套 140 项通过；随后 `header_composition` 专项增至 18 项通过，累计覆盖 141 项，并非最终重跑整套 141 项。
- 桌面库 9 项真实 UDP／数据库／XML 测试通过。GNU 测试 EXE 缺少 CommonControls manifest，复制测试 EXE 并嵌入生产 manifest 后运行通过；不宣称 `cargo test --lib` 直接通过。
- 公共头 UI 16 类检查、`header_resolution_test.cjs`、协议编辑顺序、动态数组及新增 `ui_close_guard_regression.cjs` 回归通过。完整 `ui_workspace_regression.cjs` 因缺少 `artifacts/iot-dataset-validation.db` 未运行。
- 候选 EXE 通过实际界面配置五种头选择，并由独立本机 UDP socket 收到 `A1D1`、`B1D2`、`A1B1D3`、`B1A1D4`、`D5`；A+B 报文按两套公共头与净荷分区解析。客户端 XML 重导入保留选择顺序。无人机 `0x21` 条目数 `02` 收到实际 `0x22`，两条结果为 `AA AA`。
- 候选 EXE 最大化／还原、顶部双击通过。首次关闭确认发现异步处理问题并已修改；最终 EXE 的最小化／恢复、取消关闭保留草稿、确认放弃退出通过，五种组合及无人机两条配置真实收发再次通过。
- 最终 NSIS 已重新生成：`terminal-studio/release/Simulated-Terminal_0.1.0_multi-header_x64-setup.exe`，SHA256 `0CECE92E555307919F2E02A92C815553398BD783A6307CC83F0C4F6F6E529D08`。隔离客户端入口为 `.multi-header-trial/启动公共头新版.cmd`。本轮不覆盖安装用户现用旧版，未执行安装器安装。

真实客户端证据：`terminal-studio/artifacts/multi-header/native-report.json`。使用步骤、测试边界及最终产物见 `terminal-studio/docs/公共头组合与窗口整合验收.md`。未执行大规模压力测试。

## 2026-10-19 交互控制台紧凑表单

普通字段、公共头和数组条目改为按容器宽度自动分列，最小列宽 168 CSS px；默认 HEX 不再逐字段重复提示，保留字节长度及十进制／文本类型标识。字段顺序、输入值和编码规则不变，长字段保留整行。

- `npm run build`、Windows release EXE 和 NSIS 构建通过；动态数组回归及控制台条件分支／折叠保值回归通过。
- 使用现有无人机协议检查 1700／1366／1000／760 CSS px 窗口，无页面横向溢出或脚本异常；公共头分别显示 7／5／3／2 列。数组、输入修改和字典下拉已检查。
- 最终原生 EXE 中普通字段显示 5 列、约 195 CSS px 宽，重复进制提示为零。新版使用独立数据快照，入口 `terminal-studio/.compact-form-trial/启动紧凑表单新版.cmd`。
- 安装包：`terminal-studio/release/Simulated-Terminal_0.1.0_compact-form_x64-setup.exe`，SHA256 `87017AA68C8905AC98E1CF5775F10BA88835A152A61525402ACE36EE494F3166`。本轮只调整表单展示，未重复 UDP 压测或执行安装器安装。
- 证据：`terminal-studio/artifacts/compact-form/layout-report.json` 和同目录截图。

## 2026-10-19 协议配置表单布局与焦点裁切

修复首行输入框贴住滚动容器导致外扩焦点描边被裁切的问题。基础信息改为上方标签，宽窗口五列；自动应答与定时发送成对排列，延迟／间隔使用短输入。单个公共头省去重复排序行，多头排序继续保留。字段表表头和输入列对齐，默认值列至少 128 CSS px，窄窗口只在字段表内横向滚动。

- TypeScript/Vite、Windows release EXE 和 NSIS 构建通过。多公共头 16 类回归、协议结构／数组／分支顺序回归通过。
- 1366×690、1024×700、1360×860 界面检查通过：焦点描边有余量、保存可见、页面及弹窗无横向溢出；窄屏默认值输入由约 26 px 增至约 134 px。切页草稿保留、保存字段值不变。
- 最终 EXE 使用独立库实际打开／保存／重新打开无人机配置协议，24 个字段的名称、长度及默认值保持一致，页面异常 0。常用窗口弹窗外层无额外纵向滚动，字段编辑区约 354 CSS px 高。
- 新版入口：`terminal-studio/.protocol-form-trial/启动协议表单新版.cmd`。安装包：`terminal-studio/release/Simulated-Terminal_0.1.0_protocol-form_x64-setup.exe`，SHA256 `10A53F228C3EB412180692B3BA1CDEBD35F36570ADF63DA86B9677649CC70814`。
- 证据：`terminal-studio/artifacts/protocol-form/layout-report.json` 和深浅色／窄屏／原生客户端截图。本轮不改通信协议，不重复压力测试，未覆盖安装旧版。


## 2026-10-19 字典选择与跨标签页刷新

修复协议页保留旧字典目录，导致新增的“是否”“线路号码”不可选；普通字段、数组子字段、公共头共用搜索选择器，按本组／全局分组，显示条目预览和同名 Key，其他分组收起并说明不可绑定原因。数组绑定同步长度与默认值，保留草稿及现有作用域规则。

- 前端构建、目录刷新异步／失败／事件检查、全界面新增字典与三处绑定回归、公共头 16 类及协议结构回归通过；深浅主题／1280 窗口无弹窗越界。
- 最终 Windows EXE 使用独立 SQLite 快照，通过实际 UI 新建字典条目、跨页绑定、保存重开；用户两张字典正确恢复，页面错误 0。未写入用户原库。
- NSIS：`terminal-studio/release/Simulated-Terminal_0.1.0_dictionary-picker_x64-setup.exe`，SHA256 `81CB31CF2B4265D118E940EB2784E807FC1AD4C4A03CCA02C7BE36EC9FF38242`。预览 `.dictionary-picker-trial/启动字典新版.cmd`。未运行安装器，未重复 UDP 压测。
- 详细边界与证据：`terminal-studio/artifacts/dictionary-picker/验收记录.md`、`native-report.json` 及同目录截图。


## 2026-10-19 标签页右键菜单与底部选中线

增加关闭当前／其他／左侧／右侧／全部，右击非活动标签保持当前页面；批量未保存统一确认，取消保留全部草稿，关闭全部回总览。支持键盘菜单和焦点恢复，选中强调线移至底部。

- 前端、Windows Release EXE、NSIS 构建通过；10 组标签菜单回归与现有单标签关闭确认回归通过。
- 最终原生客户端使用独立库验证实际右键、批量关闭、取消保值、键盘与底线；协议／通道／字典内容未改变，页面错误 0。
- 安装包 `terminal-studio/release/Simulated-Terminal_0.1.0_tab-menu_x64-setup.exe`，SHA256 `E8B82F0C49386513514432D3768F0EA33F682629A2CF4F122766361692E0F265`。入口 `.tab-menu-trial/启动标签菜单新版.cmd`。未覆盖安装现用客户端。
- 证据：`terminal-studio/artifacts/tab-context-menu/验收记录.md`、`native-report.json` 及同目录截图。

## 2026-10-19 标签右键菜单紧凑布局与图标

菜单由 176 缩至 128 CSS px，选项行高 28 CSS px；关闭当前／其他／左侧／右侧／全部均增加图标，保留原有关闭行为和底部选中线。

- 前端、Windows Release EXE、NSIS 构建通过；最终 EXE 深浅主题实测 128 CSS px 宽，5 个图标均为 14×14 CSS px，文字完整、Escape 恢复标签焦点，页面错误 0。
- 独立浏览器复核禁用项、主题和焦点通过。既有菜单回归本轮前 8 项通过，随后即时焦点断言遇到关闭动画时序；等待动画结束后的独立焦点检查通过，未宣称本轮整套 10 项重跑通过。
- 安装包：`terminal-studio/release/Simulated-Terminal_0.1.0_tab-menu-compact_x64-setup.exe`，SHA256 `E6992C0BCA9C3BE0B8F26BFB6E6A821A4A9BCEB701728D7AFCCB7E8218BFA2ED`。预览使用独立备份库，入口 `.tab-menu-compact-trial/启动紧凑菜单新版.cmd`；未覆盖安装现用客户端。
- 证据：`terminal-studio/artifacts/tab-menu-compact/native-report.json`、`visual-metrics.json` 和深浅主题截图。

## 2026-10-19 XML 编辑器重构

以本地打包的 Monaco 替换透明 textarea/pre 叠加编辑器，统一输入、光标、高亮、行号及滚动，增加查找替换、折叠、换行和错误定位。独立弹窗保存草稿；停止输入后校验，应用时复核最新模型。不更改 XML 导入格式或协议业务规则。

- 旧版已复现独立 `<` 在高亮层丢失，以及 Tab 后立即输入导致首字符落到文末。新版组件 7 组回归通过，覆盖第 780 行、横滚、中文 IME、撤销重做、查找替换／折叠、错误定位及深浅／窄屏；末次 `*.xml` 配色调整后已重跑。校验延迟实测约 307.7 ms，无浏览器异常。
- 页面集成 10 项通过：错误禁止应用、最新文本提交、切页保稿、Escape 不误关、写入／刷新失败保稿、成功重开、模型释放、空导出及列表未加载时打开。
- Windows release EXE 使用独立 SQLite 快照实际编辑、应用并重新打开 42 条协议／43 张字典的 788 行 XML；还原后的导出与原始文本完全一致。非法 XML 禁用应用并可定位。1360×860 窗口只渲染 25 行，无页面异常；资源仅来自本地 Tauri／IPC 主机。未修改用户原数据库。
- 前端、Windows release EXE 构建通过；NSIS 已生成至 `terminal-studio/release/Simulated-Terminal_0.1.0_xml-editor_x64-setup.exe`，本轮未运行安装器或进行升级安装，不重复 UDP 压测。独立预览入口 `.xml-editor-trial/启动XML编辑新版.cmd`。
- 末次配色后的最终 EXE 已复核深浅主题及第 760 行对齐；788 行仅渲染 25 行，导出与原始 XML 一致，页面错误为 0。最终 NSIS SHA256：`FE93D949B512B74FFB87D8EE2AF057D9DCFD1E4AC8DD703B8A07AE4D1C3F85CB`。
- 证据：`terminal-studio/artifacts/xml-editor/验收记录.md`、`results.json`、`integration-report.json`、`native-report.json` 及同目录最终 EXE 深浅主题截图。

## 2026-10-19 列表边距、表框与滚动统一

修正协议筛选栏无内边距、表格 8 px 与分页 12 px 不一致、协议／字典嵌套滚动，以及设备分页与表框分离。共用表格统一 12 px 边距和固定表头，筛选放入同一表框；公共头字段表共用列轨道，8 列横坐标一致。

- 前端、Windows release EXE、NSIS 构建通过。4 组列表回归覆盖 18 个尺寸／主题布局及筛选、分页、横纵滚动；设备表额外验证展开、空态与长列表；页面错误均为 0。
- 最终原生 EXE 在独立快照验证 42 条协议、52 张字典、4 个通道；应答筛选 11+3 条翻页正确，表头／正文／分页左右均 12 px，业务数据前后相同。未修改用户原库、未运行安装器或 UDP 压测。
- 安装包 `terminal-studio/release/Simulated-Terminal_0.1.0_table-layout_x64-setup.exe`，SHA256 `E8E8708592D054BD23BA9B624746D4918E1CDEA610C2ADB9949FBDE144492138`。
- 证据：`terminal-studio/artifacts/table-layout/验收记录.md`、`before.json`、`results.json`、`device-results.json`、`header-grid.json`、`native-report.json` 及截图。
## 2026-10-20 总览通信统计图表

总览底部增加收发趋势与各通道报文量，支持最近 1 小时、24 小时、7 天。Rust/SQLite 汇总真实持久化日志，空时段补零，活动页面每 5 秒刷新，可手动刷新；监控清屏或暂停不影响历史统计。

- 核心聚合测试 2 项、UI 检查 5 组通过；前端、Windows release EXE、NSIS 构建通过。
- 最终 EXE 使用用户数据一致性备份验证，7 天接收 44 条、发送 57 条；三种时段的逐桶、逐通道及总数均与独立 SQL 查询一致。深浅主题与 1360×860 页面检查通过，无页面异常或横向溢出。
- 安装包 `terminal-studio/release/Simulated-Terminal_0.1.0_dashboard-charts_x64-setup.exe`，SHA256 `3856BA5D199CFF2BF04ECF06EDFBEF979934DA2DD4624B0859A7D5BC4145A3F7`。未运行安装器、未覆盖现用客户端、未进行 UDP 压测。
- 详细证据及桌面测试运行器限制见 `terminal-studio/artifacts/dashboard-charts/验收记录.md`。

## 2026-10-20 总览紧凑布局

顶部统计卡片由约 130 px 降至 82 px，辅助统计与数值同行，收紧总览分区间距和中间卡片内边距。

- 前端、Windows release EXE、NSIS 构建通过。最终 EXE 默认 1360×860 窗口、4 通道数据下，深浅主题 × 三个统计时段共 6 组布局检查通过；main 可视高度与内容高度均 818 px，无横纵滚动条，图表完整，页面异常 0。
- 未强行隐藏滚动条；额外数据或缩小窗口时仍可滚动。未覆盖现用安装或用户原库。
- 安装包 `terminal-studio/release/Simulated-Terminal_0.1.0_dashboard-compact_x64-setup.exe`，SHA256 `45A886C4582F46D4A6545B6EE68DA09D0079D8D891BACC8D18638662BAB600D3`。
- 证据：`terminal-studio/artifacts/dashboard-compact/验收记录.md`、`native-check.json`、深浅主题截图。
## 2026-10-20 仓库整理与独立构建

- 将当前 `terminal-studio` 工程纳入 Git，提交已被替换的 `musp-studio` 删除记录。保留原开发文档、七张原型、协议数据和回归脚本；备份、临时预览、数据库、构建缓存、测试产物及安装包不纳入源码仓库。
- 保留示例生成器使用的原始协议定义至 `docs/06_示例与测试数据/musp-v08-reference.xml`，字节校验与旧文件一致；生成脚本不再依赖备份目录。
- 移除旧基座绝对路径链接配置，使用工程内 WebView2 SDK，并由 vendor 构建脚本在输出目录生成 GNU 静态库名称。修复后 Windows Release 构建通过；临时空数据库启动检查通过，客户端正常显示并能调用 Rust 统计接口。
- Rust 核心测试、TypeScript 检查、34 条协议样例的长度/CRC/golden 校验通过。暂存文件未发现数据库、EXE、node_modules、target、测试截图或凭据模式命中。
- 本地旧备份、预览及测试产物的物理删除被执行工具的自动审批阻止；批量和单目录递归删除均被拒绝，返回信息未提供更具体原因。这些文件仍在本地，但已排除出 Git 提交。本次未覆盖安装或改动用户运行数据库。
