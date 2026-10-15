# MUSP Studio — 完整开发文档（合订版）

本文件是便于全文检索/导入 Codex 的合订本；**各分卷MD与原始MUSP v0.8协议另行包含在ZIP中**。



---

<!-- 来源文档: 00_开始阅读/README_交付说明.md -->

# MUSP Studio — Windows 多系统、多协议 UDP 模拟平台

**交付类型：** 产品与开发设计包（不是已经编译好的 EXE，也不是可直接运行的前端源码）  
**交付基线：** MUSP v0.8 业务讨论稿（原样保留）  
**设计冻结日期：** 2026-10-14  
**目标系统：** Windows 10/11 x64，可直接安装的 `.exe` 安装程序；无需部署服务器和独立数据库。

## 从哪里开始

1. `01_需求与设计/01_产品需求规格书.md` — 功能边界、用户工作流、验收。
2. `01_需求与设计/02_页面原型与交互规格.md` — 7 张原型的对应模块、实际字段、操作细节。
3. `02_协议与数据模型/03_XML协议DSL与在线编辑.md` — 主要业务难点：公共头、字段结构、复用、位域、动态数组、在线编辑。
4. `02_协议与数据模型/04_流程引擎与业务场景.md` — 多设备并发、等待队列、注册、通信、配置、资源释放。
5. `02_协议与数据模型/05_本地数据模型与持久化.md` — SQLite 表及状态恢复策略。
6. `03_开发与测试/06_技术架构与工程规范.md` — 固定技术栈、项目目录、Rust/React 模块边界。
7. `03_开发与测试/07_测试计划与验收标准.md` — 可执行测试标准及发布检查。
8. `03_开发与测试/08_迭代计划与Windows安装包.md` — 阶段实现顺序、NSIS/离线 EXE。
9. `04_Codex提示词/00_总控开发提示词.md` — 首次交给 Codex 的主提示词；其余为分阶段补充。

## 原型图

`05_原型图/` 存放 7 张独立的 1586×992 PNG：工作台、协议设计、设备管理、交互流程、报文监控、通话资源、模板与 XML 在线编辑；另有索引总览图。

**重要：** 原型图是视觉方向参考，不是可靠的业务数据样本。有的图片包含虚拟日期、示意报文、非实际信令码和不严谨字段长度。Codex 不得从图片反推通信协议和后台逻辑；业务事实以 MUSP v0.8 原始协议以及本包中的确认与勘误为准。

## 业务确认的不可变约束

- 1B 信令码、2B 会话 ID、3B 源/目的设备地址，MUSP 基础公共头 23B。
- 协议表列：序号、字段名称、字段长度（Byte）、字段类型（HEX）、说明，**不列偏移**；报文解析结果可以显示实际字节位置、HEX、DEC、BIN、中文解释。
- 设备状态仅 OFFLINE `00`、ONLINE `01`、COMMUNICATING `02`，其他等待/清理状态仅属于流程或资源。
- `0x22` 配置响应业务体只包含 1B count + N×1B result；`AA`=对应条目成功。例 `05 AA AA AA AA AA`。
- `0x21` 请求可选用 8B 配置通用段；该段里 Bit7 原子性、Bit0 结束标识为**讨论方案**，其他位与批次/分段字段待真实协议确认。
- 设备既支持手动管理，也支持配置报文映射生成；动态数组既能表示多台设备，也能表示一设备多组参数。
- 请求响应匹配：信令码初筛 + 设备/端点/事务/会话关联；每条等待项拥有超时、重试、迟到包处理。
- 视频：A 申请→控制系统寻呼 B→B 应答→控制系统响应/通知→A、B 分别 RX/TX 资源配置；语音：A 申请→控制系统响应→A 分别 RX/TX 配置。带宽需求在申请时提出，实际带宽由 `0x70/0x71` 单独下发。
- 通信启动既可本地资源就绪即生效，也可等待控制系统统一就绪通知，按协议选择；结束时通知各方，按资源实例撤销并确认。
- 无登录、不需要云端服务；允许离线本地操作和远程 UDP 对端通信。

## 文件优先级与差异处理

1. **用户后续明确确认的业务规则**（本 README 与 `01_产品需求规格书.md` 的“已确认”内容）。
2. `07_原始协议讨论稿/MUSP_v0.8_协议业务基线_原文不修改.md` 的报文定义、信令码和具体业务细节。
3. 本包的技术方案、示例 XML、样例流程（属于设计建议和需要实现的示范，不冒充真实行业协议）。
4. 视觉原型图片（最低业务优先级）。

如果相互冲突，不要默默“修正”来源，而应建立勘误记录，在实现中采取上述优先级并补单元测试；无法确定的字段设置为待配置，不得伪造具体含义。

## 推荐项目名称与目录

- 软件名称：`MUSP Studio`
- 英文工程目录：`musp-studio`
- Windows 安装包期望：`MUSP-Studio_<version>_x64-setup.exe`
- 数据目录：`%APPDATA%\\MUSP Studio\\`（实际用 Tauri app-data 目录 API 获取）

## 交付的范围说明

本次交付：可独立检查的开发说明、原型图、样例协议 XML、测试向量生成脚本、Codex 分阶段提示词。\
本次**不包含**：已经实现的运行引擎、编译 EXE、经过设备实测的标准协议或经过逐屏视觉验收的交互应用。



---

<!-- 来源文档: 01_需求与设计/01_产品需求规格书.md -->

# 01 产品需求规格书（PRD）

## 1. 目标

打造 Windows 本地安装型 MUSP Studio：用户无需编写代码就能为不同系统建立 UDP 协议、定义报文公共头/字段/枚举/Bit、编排任意交互流程、配置设备、模拟收发并追踪结果；高级用户可直接在线编辑 XML 协议文件并复制/复用字段结构。适用于通信联调、设备模拟、协议回归测试、复杂异常注入，不依赖服务器。

## 2. 用户角色与典型任务

- 协议设计人员：从 XML/手工建协议，定字段、导入模板，校验长度/编码/解析；基于已有协议复制类似信令。
- 联调人员：建立系统 A/B，关联真实 UDP 对端，手工创建或通过报文创建设备，模拟注册/心跳/配置/视频/语音/结束。
- 测试人员：批量创建设备、控制发送间隔、丢包/延时/超时重试、按报文/设备/流程追踪、保存测试场景和报告。
- 本地单用户，无登录账号、在线会员、云管理等非核心模块。

## 3. 功能清单（F = 功能 ID，P0/P1/P2 为阶段优先级）

| ID | 优先级 | 功能域 | 必需行为与验收入口 |
|---|---|---|---|
| F01 | P0 | 工作区/项目 | 创建/打开/关闭本地项目，切换系统/场景；自动保存草稿，明确已发布版本 |
| F02 | P0 | 系统/参与者 | 一个项目多系统，每个系统绑定多个协议和多个逻辑设备；本地/远端参与者角色分离 |
| F03 | P0 | 端点管理 | 多个 UDP 服务实例独立启动停止，支持 bind IP/port、本地网卡、远端 endpoint、发送端口 |
| F04 | P0 | 协议管理 | 创建/复制/导入/导出/版本化 XML 定义，复用公共头和结构片段 |
| F05 | P0 | 协议字段编辑 | 字段、结构体、数组、条件字段、枚举、Bit、长度绑定、字节序、计算字段、校验码 |
| F06 | P0 | XML 在线编辑 | Monaco 编辑、复制行/节点/结构、格式化、撤销恢复、实时诊断、发布前编译校验、源码与表单双向同步 |
| F07 | P0 | 设备管理 | 手动设备/协议配置结果生成设备；协议身份（地址 3B）和网络端点分开；离线/在线/通信三态 |
| F08 | P0 | 报文编辑/收发 | 选择协议及设备，表单输入业务字段，动态生成 HEX；发送单次/循环/定时/条件触发 |
| F09 | P0 | 报文解析 | HEX 原始/中文字段/包内位置/DEC/BIN/枚举/Bit/动态列表/校验错误/反向定位 |
| F10 | P0 | 日志 | 收发报文、相关设备、信令名、事务、HEX、失败原因、暂停滚动、过滤、导出 |
| F11 | P0 | 流程设计 | 拖拽发送/等待/条件/变量/循环/并行/子流程/设备更新/超时异常节点；连线及持久化 |
| F12 | P0 | 流程运行 | 模板+场景=多独立运行实例；收到符合信令码与事务/设备/端点的响应唤醒对应流程 |
| F13 | P0 | 会话变量 | 系统/设备/会话/流程/步骤五级；Token 自动提取、保存、过期和信令字段自动绑定 |
| F14 | P1 | 模拟批量设备 | 多设备并发注册、状态流转、速率调节、来源过滤、批量停止 |
| F15 | P1 | 配置批次 | `0x21` 可选 8B 段、5×10 动态数组、`0x22` count+逐条 `AA` 响应，支持设备/参数两种映射 |
| F16 | P1 | 视频/语音通话 | A/B 寻呼或单端申请、所需带宽申请、许可、控制系统各方向资源下发与确认 |
| F17 | P1 | 通话与带宽 | callId、申请/预留/下发/生效/未知/回收资源独立记录，端主动/控制主动结束与离线清理 |
| F18 | P1 | 测试编排 | 模拟超时、丢包、重复、延时、错序、坏 CRC、异常值及重放；可配置故障注入作用域 |
| F19 | P1 | 模板库 | 公共头、字段片段、Bit 位组、结构体、整包、业务流程可收藏/克隆/导入导出 |
| F20 | P1 | 数据导出 | HEX/TXT/JSON/CSV 日志，项目 ZIP 导入导出，协议 XML 导出，测试结果报告 |
| F21 | P1 | Windows 安装 | NSIS 安装 EXE，正常卸载，不依赖外部服务；Windows x64；可离线安装 |
| F22 | P2 | 图形化诊断 | 设备拓扑图、通信资源趋势、差异对比、设备大规模测试场景 |
| F23 | P2 | 插件扩展 | 自定义协议解析/校验函数受控插件接口，不直接执行用户脚本 |

## 4. 项目、系统、协议、设备、场景的关系

Project(1) → System(N)；System ↔ Protocol（多对多，使用明确版本绑定）；System(1) → DeviceInstance(N)；DeviceInstance(1) → ProtocolIdentity(N)；Scenario → Participants / Endpoints / WorkflowVersion；WorkflowVersion(1) → WorkflowRun(N)；WorkflowRun ↔ Device / Packet / PendingRequest / Session；Call(1) → CallParticipant(N) → Resource(N)。

**重要：** 本地通信端点是 IP:port 与 socket；协议设备地址是逻辑地址；设备唯一 ID 是本地数据库 UUID。三者不能混成一个字段。

## 5. 已确认协议业务约束

- MUSP 示例公共头固定 23 字节：信令码 1 字节、会话 ID 2 字节、源/目标逻辑地址各 3 字节；可选认证头扩展在 23B 之后。
- 字段定义表一律显示：序号、名称、长度（Byte）、字段类型 HEX、说明；字节位置仅用于报文**解析视图**。
- 设备外显状态严格为 `OFFLINE(00)` / `ONLINE(01)` / `COMMUNICATING(02)`；设备创建默认离线，注册完成后在线，成功建立通信后通信，最后一条通信结束回在线，掉线/主动离线直接离线。
- `CONFIG_RESULT(0x22)` 专用业务体严格 `itemCount(1B)+results[N](每项1B)`；`AA` 表示对应条目成功，其他结果含义未定；不能增加总结果/失败编号到该业务体。
- `CONFIG_SET(0x21)` 专用前缀10B，动态 `itemCount × itemSize`；可在其前有可选8B configCommon；是否使用按信令配置。8B 结构的 Bit7/Bit0 是现阶段候选定义，保留其余字段待确认。
- 配置既支持一个 item 创建一台设备，也支持一台设备的多组参数；映射生效策略按协议/场景配置。
- 申请带宽在 VIDEO_CALL_REQ/VOICE_CALL_REQ 中；资源通过 `0x70 RX` 与 `0x71 TX` 分别下发，`0x72` 分别确认。
- 通话启动模式 LOCAL_READY 或 CONTROL_NOTIFY (0x75)，由系统协议配置；通信结束要逐个参与方、逐个 RX/TX 回收，不可依靠 UDP 发送成功判定已释放。
- 同一设备可有多个独立通话（由设备类型限制最大并发）；回收一条不能影响其他 callId。
- 每条需要响应的信令发送前创建 PendingRequest，先由期望响应信令码筛选，再按协议版本、通信端点、源/目的设备、事务号、会话规则进行唯一匹配。

## 6. 主用户旅程

**旅程 1 / 构建新协议：** 新建项目 → 新建系统 → 新建协议 → 选择或复制公共头 → 定义信令码 → 新增配置通用8B结构 → 编辑动态字段 → XML源码编辑 → 校验并发布协议版本 → 本地构造/解析同一测试包。

**旅程 2 / 设备配置：** 手动导入设备或创建配置发送模板 → 生成动态数组 N×10 → 发 `0x21` → 收 `0x22` → 按索引关联结果 `AA` → 按模式创建设备或更新设备参数 → 新建设备默认离线。

**旅程 3 / 注册认证：** 选设备 → 开机注册 → 发送前登记响应等待 → 收许可 → 可选认证信息下发保存会话 → 注册确认 → 收注册完成 → 状态 ONLINE。

**旅程 4 / A-B 视频：** A 申请 TX/RX → 控制端寻呼 B → B 应答 → 控制端响应 A、通知 B → 给双方分别下发 RX/TX 并确认 → 本地启用或控制端统一就绪 → 双方 COMMUNICATING。

**旅程 5 / 单端语音：** A 申请带宽 → 控制端许可 → 分别 RX/TX 配置与确认 → 按模式启动通信；结束后若无其他通话则 ONLINE。

**旅程 6 / 结束/掉线：** A/B/控制主动结束或心跳判离线 → 终止对应通话 → 通知另一方 → 对各方资源撤销并确认 → 已确认释放回池；UNKNOWN 不回池；掉线设备 OFFLINE。

## 7. 核心非功能需求

- 无公网依赖；离线安装、不联网也可新建项目与加载示例；不需要 MySQL、Redis、Java 或 Node 运行环境。
- 运行中没有 UI 阻塞：网络 IO、协议编解码、流程调度全在 Rust 后台执行；前端只接收节流的状态和日志增量。
- 协议文件是用户资产：可备份、版本化、导出、导入、比对；不可因升级静默覆盖。
- UDP 不保证可靠、顺序或交付；明确展示发送成功、匹配响应、业务成功这三类独立结果。
- 默认最小权限：禁止 XML 外部实体、不执行外部脚本、保留位仅按 schema 处理；Token 日志脱敏。
- 可恢复：数据库 SQLite WAL，发布的 schema/flow 使用快照；异常重启后运行实例标记 interrupted，不无条件重发危险命令。
- 目标性能先定义可重现实测门槛，见测试计划，不用原型展示的虚构吞吐量做验收。

## 8. 非目标与不确定项

- 本软件不是音视频编解码器/播放器；当前模拟的是**控制与资源信令**。若将来模拟媒体流传输，需要新模块。
- 对真实外部 UDP 协议的兼容性以用户导入的准确字段定义为前提，不能假设任何格式都自动识别。
- 8B 配置通用段除 Bit7/Bit0 的暂定方案外，其余字段有“待确认”标记；不写死真实行业语义。
- 认证强度、带宽分配算法、资源池拓扑和报文加密由具体外部系统协议提供；MUSP 仅为验证样例。



---

<!-- 来源文档: 01_需求与设计/02_页面原型与交互规格.md -->

# 02 页面原型与交互规格

## 0. 原型资产清单

| 页面编号 | 原型文件 | 主用途 |
|---|---|---|
| UI-01 | `01_工作台总览.png` | 项目/系统/场景切换、快捷执行、三态设备与收发摘要 |
| UI-02 | `02_协议设计器.png` | 公共头、通用段、信令字段、HEX/Bit/数组编辑 |
| UI-03 | `03_设备管理.png` | 按系统管理设备实例、3B 协议地址、配置项、来源历史 |
| UI-04 | `04_交互流程编排.png` | 可视化节点、条件/等待/超时、变量与运行追踪 |
| UI-05 | `05_报文监控解析.png` | 实时日志、原始 HEX、字段树、包内字节定位 |
| UI-06 | `06_通话与带宽资源.png` | callId、RX/TX、资源生命周期、回收异常 |
| UI-07 | `07_模板库与XML在线编辑.png` | XML/可视化双编辑、复制结构、验证、发布与复用 |

**视觉验收边界：** 原型图片仅约束“暗色专业工作台/信息组织/交互动线”。示意文本、日期、模拟吞吐量和无关标签若与 MUSP v0.8 不一致，必须由已确认的业务模型替换。拒绝把页面堆成门户大屏或过密的管理后台。

## 1. 全局框架

- 桌面标题栏：项目选择、当前场景、启动/停止、深/浅主题切换（单击即时生效，无二次确认）、项目状态、窗口控制。
- 一级导航：工作台、协议设计、设备管理、交互流程、报文监控、通话资源、模板库、系统设置。
- 顶部工作区 Tab：可打开多个协议、信令、设备、流程。未保存标记与关闭确认；支持 Ctrl+S。
- 控件密度：主操作最大程度减少弹窗；高频编辑采用表格/侧栏，少用层层弹窗；1600×900 起布局可用，1366×768 不允许关键按钮被遮挡。
- 左侧导航可折叠；列表/编辑区之间可拖动宽度；用户调整持久化。

## 2. UI-01 工作台

**区块：** 当前项目/场景 → 设备三态统计（离线、在线、通信）→ 活动流程数量 → 待响应数量 → 当前端点状态 → 最近收发报文。设备拓扑仅为辅助视图，可切换列表。

**用户动作：** 选择场景、一键启动/停止、查看执行明细、快速进入协议或设备编辑、创建新的 UDP 监听实例；启停必须准确反映 socket 状态。

**避免：** 使用“系统正常运行”代替真实网络监听成功；不存在设备时不能虚构设备。

## 3. UI-02 协议设计器（最核心）

左侧协议结构树：`Protocol → SharedFragments → Messages → Sections → Fields / Structs / Arrays / Bits`；中央字段表及预览；右侧属性面板与验证消息；底部 XML 源码/JSON AST 视图和示例 HEX。

**字段主表固定五列：** 序号｜字段名称｜字段长度（字节）｜类型（HEX）｜说明。可额外有“操作”图标区，但不作为协议表数据列，不显示偏移。内部属性在右侧：`key / labelZh / lengthExpr / endian / valueKind / signed / enumMap / bitRange / condition / binding / validation / scale / unit`。

**必备编辑动作：**

- 新增字段、新增结构体、重复数组、条件组、位域、固定值、校验字段；上移下移、拖拽排序。
- 在同一层复制字段 Ctrl+C/Ctrl+V，在其他信令粘贴时自动生成稳定唯一 key/id；复制字段组时保留内部引用并相对重映射。
- “复用引用”与“复制一份”两个独立动作：引用后修改共享模板影响所有引用；复制后的修改只影响自身。
- XML 在线编辑不经过云端：文本输入、查找/替换、格式化、诊断、复制整段、新建模板；切换可视化之前先解析并提示不可逆/未保存编辑。
- 动态数组当 `count=05` 且 `itemSize=0A` 时显示 5 个 10B 条目，合计 50B；改 count 自动调整表单；协议定义时可先不填真实条目值。
- 控制标志 `81` 展开 Bit7=1（原子性）、Bit0=1（结束），其余位按规则标待确认；允许单击编辑 bit 后回写对应 HEX 字节，反之亦然。
- 8B 配置通用段为可复用 fragment；每条信令可以独立启用/关闭；自动维护 bodyLength 和 CRC。
- 编码与解码共享 schema：改变 XML 发布版重新编译，立即运行样本 encode/decode roundtrip 测试。

**编辑状态：** `Draft` → `Validating` → `Validated` → `Published`；运行中的 flow 永远绑定某个已发布版本快照。错误 XML 不得发布，但应保留错误草稿与撤销历史。

## 4. UI-03 设备管理

左侧系统与设备树；中间设备实例列表与筛选；右侧设备详情 Tab（基本信息、协议身份、配置参数、通信会话、资源、历史）。

- 状态仅离线、在线、通信；不展示“注册中”“清理中”作为第四种设备状态，流程运行状态另行展示。
- 创建设备可选“手动配置”“从报文应用”；记录来源 MANUAL/PACKET、来源信令、版本、变更前后快照。
- 设备协议地址以 3B HEX 编辑：例如 `00 00 3B`，提供单字节分格或连续 HEX；不与内部 UUID 混淆。
- 系统下自动汇总设备；在其他信令模板中选择设备后自动绑定地址、会话、配置数据；同时允许同一设备有多协议身份。
- 设备参数可使用重复结构表单或复制现有参数组；异常或未定义配置保持原始 HEX 并提供编辑但不擅自解释。

## 5. UI-04 可视化流程编排

画布：新增节点、放大缩小、画布适配、节点吸附、连线分支标签；右侧“基础/匹配/超时/输出/错误分支”节点属性；底部可折叠变量与执行追踪。

节点类别：Start/End、Send、WaitPacket、SendAndWait、Condition、Assign、DeviceUpdate、Delay、Loop、Parallel/Join、Subflow、EventListener、Failure；禁止无限循环未限速。

**重点交互：** 用户为“注册请求发送”配置期望 `0x02`，等待队列显示每个设备当前待响应码、transactionId、deadline/retry；命中后立即高亮成功分支。支持“只匹配信令码”“信令码+设备”“信令码+事务+设备+会话”等模式，但并发无法唯一匹配时应报错或限制并发。

画布左侧配置/布局存磁盘；流程版本发布前检查引用协议/字段是否存在、分支是否悬空、循环是否有退出与次数限制、并行 Join 是否闭合。

## 6. UI-05 报文监控解析

三栏主要布局：报文日志表（方向、时间、系统/设备、码值、事务号、长度、状态）｜原始 HEX 十六字节网格+ASCII｜字段树/表格与 Bit 检视。窗口可调整、折叠、虚拟滚动。

**解析表建议列：** 序号、实际字节位置（支持 1 起始/0 偏移切换）、字段名称、长度、HEX、十进制、二进制、中文含义、校验；这个位置列只用于解析结果，不进入协议定义表。

- 选中 HEX 单元 → 高亮字段路径；选中字段 → HEX 滚动定位、范围高亮；嵌套数组可展开 `items[0]...items[4]`。
- 有符号补码正确换算；HEX `FF FE` 有符号 16-bit = -2；无符号则 65534；ASCII/BCD/缩放/单位由 schema 元数据决定。
- 原始未知报文仍可查看 HEX 和来源，但必须显示“协议未知/解析失败”，而不是伪造中文解释。
- CRC、长度、数组越界、未知枚举、保留位异常明确区分；Token 等敏感数据默认遮罩且日志导出亦脱敏。
- 发送侧同样可查看组包过程、输入字段与生成字节的映射。

## 7. UI-06 通话与带宽

通话列表（callId、类型、发起/被叫、申请 RX/TX、许可状态、就绪/业务中/结束中）；详情区显示 A 和 B 参与者、各自 RX/TX 资源实例、资源 ID、版本、频点/时隙/带宽以及确认历史。

- 可按正常 A 挂断/B 挂断/控制端结束/设备离线演练。
- RX 与 TX 独立确认，缺一不可时通话不可标全局 READY；LOCAL_READY 与 CONTROL_NOTIFY 用场景属性切换。
- 通话业务停止与回收状态独立：UNKNOWN 资源不得自动回池；显示清理中、失败及审计，但不影响设备只显示三态。
- “强制清理”操作必须要求用户明确确认（因为会修改业务状态），不允许仅凭超时显示回收成功。

## 8. UI-07 模板库与 XML 编辑

三栏：分类树（公共头/字段片段/位域组/配置段/完整信令/完整协议/流程模板）｜可搜索列表｜可视化结构与 XML 源码；右侧实时诊断可抽屉展开。

**复制/重用承诺：**

- Ctrl+D 当前字段复制；Ctrl+C/V 复制粘贴文本或结构；支持批量选择多个字段并复制。
- “从现有信令克隆”自动拷贝字段/位域/默认值/约束，生成新的 msgCode/id，避免覆盖原文件。
- 可将重复结构保存为 fragment，然后其他协议消息 `<use ref="..." />`；被引用的公共模板变动时运行版本快照不受影响。
- XML 源码改完后进行语法校验 + Schema/DSL 语义校验 + roundtrip 样例校验；校验错误定位行/列与字段路径。
- 导入/导出单协议 XML、多个模板 ZIP；在 XML 原样编辑与可视化 AST 间处理注释保留：保留用户注释与未知扩展属性，或者在不可保留时警告并拒绝自动格式重写（详见 DSL 文档）。

## 9. 主界面快捷键

Ctrl+N 新建；Ctrl+S 保存草稿；Ctrl+Shift+S 发布协议版本（需校验通过）；Ctrl+D 复制当前字段/节点；Ctrl+F 查找；Ctrl+Shift+F 项目内查找；Ctrl+Z/Y 撤销重做；Delete 删除选中项（危险操作前确认）；Space + 拖动 平移流程画布；Esc 退出编辑。

## 10. 原型校正说明

- 当前 PNG 中有示例性信令码 `0x2101`、`0x3201`，与 MUSP 当前的 1B 信令码不符，必须替换为 `0x21`、`0x60`、`0x70` 等 1B 范围。
- 当前 PNG 示例设备协议地址存在 1B 或非 3B 形式，与 MUSP v0.8 不符；正式应用中按 3B HEX 格式处理。
- 原型中的“XML 编辑器”展示的字段类型如 uint8 属于 UI 概念；正式 XML 主协议表全部显示 HEX，数值解码语义保留在字段属性中。
- 原型的登录头像/管理员按钮属于设计渲染占位，不意味着产品有登录/权限后台；交付应用无需登录。
- 原型中使用的无人机图片、图标、示例速率和时间戳不能当真实设备和实时数据。



---

<!-- 来源文档: 02_协议与数据模型/03_XML协议DSL与在线编辑.md -->

# 03 XML 协议 DSL 与在线编辑器规范

## 1. 选择 XML 的理由与取舍

本项目的协议外部源文件**确定使用 XML**（`.musp.xml` 或 `.xml`）；UI 提供结构化编辑、XML 源码编辑、JSON AST 只读/导出视图。原因：层级明确、支持字段组及引用、方便复制类似结构、适于 Diff/版本控制。**数据传输始终是二进制 UDP，不通过 XML 传输。** XML 只描述如何编码/解码报文。

源码编辑器不依赖互联网，“在线编辑”含义是应用内直接编辑并实时预览，支持离线使用；使用 Monaco 编辑器实现，保存本地项目文件。

## 2. Schema 总模型

- `protocol`：id、nameZh、version、byteOrder、schemaVersion、signature、maxPacketBytes。
- `fragments`：可复用 `struct`、`bitGroup`、`arrayItem`、`header`、`trailer`；有稳定 `id`。
- `messages`：唯一 `code`、nameZh、direction、responseCodes（0/N）、sections 编排、configCommon8启用开关。
- `section`：`header / headerExtension / body / trailer`，body 内可以先 use `configCommon8` 再 use 专用 body；`bodyLength` 包括所有 body 段。
- `field`：`key`（英文机器名）、`labelZh`（中文名）、`lengthBytes` / `lengthExpr`、`display="HEX"`、`valueKind`（uint/int/bytes/ascii/bcd/bitset...）、`endian`、`signed`、`enumRef`、`binding`、`value/default`、`scale/unit`、`when`、`computed`。
- `array`：`countRef` 或 `countExpr`、`elementBytes` 或 `elementSizeRef`、`itemSchema` / `switch`、maxItems。
- `bits`：每个 bit/bit-range 可有 bitStart、width、valueMap；Bit 索引从 LSB=0。
- `validation`：长度、合法枚举、数值范围、必填、保留位限制、CRC16、条件字段、一致性。
- `metadata`：来源、说明、注释、待确认标记、示例。

## 3. XML 示意与实现规则

本交付包含一份机器可解析、但尚未被实际 Rust 编译器验证的**DSL 样本** `06_示例与测试数据/musp-demo.xml`。它是对解析器/编辑器的实现契约，不是现成的工业标准 XSD，不得声称已经经过执行验证。

必要片段形态示例：

```xml
<fragment id="configCommon8" nameZh="配置类通用8字节">
  <field key="configFlags" labelZh="配置控制标志" bytes="1" display="HEX" valueKind="bitset">
    <bits>
      <bit key="atomic" labelZh="原子性" start="7" width="1" />
      <bit key="end" labelZh="结束标识" start="0" width="1" />
    </bits>
  </field>
  <field key="batchId" labelZh="配置批次编号（待确认）" bytes="2" />
  <field key="segmentNo" labelZh="分段序号（待确认）" bytes="2" />
  <field key="reserved" labelZh="保留字节" bytes="3" valueKind="bytes" />
</fragment>
```

`0x21` 的 body 示意：

```xml
<message code="0x21" id="CONFIG_SET" nameZh="设备配置请求">
  <header ref="muspHeader23" />
  <headerExtension ref="authTLV" when="bit(header.flags,0)==1" />
  <body>
    <use ref="configCommon8" enabled="true" />
    <use ref="configSetPrefix10" />
    <array key="items" labelZh="配置条目列表" countRef="body.itemCount"
           elementSizeRef="body.itemSize" maxItems="20">
      <variant selector="body.configMode" equals="0x01" ref="deviceItem10" />
      <variant selector="body.configMode" equals="0x02" ref="channelItem10" />
    </array>
  </body>
  <trailer ref="crcCcittFalse2" />
</message>
```

**长度关键点：** `configSetPrefix10` 精确 10B；`configCommon8` 精确 8B；items `itemCount × itemSize`，如 5×10=50B；因此开启 8B 时 bodyLength=8+10+50=68B；关闭则 60B。`0x22` 默认为 `itemCount(1B) + results[count](1B/条)`，不隐含 8B 通用段。

## 4. 字节数值与显示语义

- 用户编辑字段值使用 `HEX` 显示，实际输入允许选“十进制/十六进制”并实时互换，但协议主表“类型”统一 HEX。
- 内部可 `valueKind=uint/int/rawBytes/bcd/ascii/bitset`，用于 DEC、BIN、中文枚举及显示单位；**HEX 字符串不是“所有字段都是无符号整数”**。
- 3B 逻辑设备地址始终编码三个字节，示例 `00 00 3B`；2B sessionId 示例 `12 34`；1B msgCode 示例 `21`。
- 所有多字节默认大端，字段可覆写；动态字段长度在 schema 编译期静态验证依赖，在运行时按已解析计数/长度求值。
- Bit 控制 `81`：Bit7=1、Bit0=1，中文解释“要求原子性/本批最后一段”，其余 Bit6..1 默认为保留；Bit7/Bit0 当前为候选业务定义，页面应支持重配置而不是写死。
- `crcCcittFalse2` 覆盖从 magic 到 body 末尾的字节，计算参数 poly=1021、init=FFFF、无反射、xorout=0；编码时填充、解码时校验。其他协议可选其他 CRC/校验方法。

## 5. 在线编辑双向同步的规则（不可省略）

UI 的“结构视图”和“XML 源码视图”共享同一个 `DraftDocument`；修改任一视图都写入同一份草稿及撤销栈。采用以下机制防止覆盖丢失：

1. XML 中编辑时保持局部文本，防抖执行 XML well-formed 解析；错误时**不强行覆盖可视化 AST**，同时继续保留源文本和具体诊断。
2. XML 编译/校验成功后生成 `ValidatedDraftAst`，允许同步可视化结构；用户点击保存只保存草稿，点击发布要再次通过全部语义检查。
3. 可视化修改通过 AST 生成 patch 并映射到 XML 节点，不允许每次都不加提示地将全文件重写从而丢失注释/格式。首版可以保留 AST+XML 的唯一规范化输出，但**转换无法保留注释/未知扩展时必须警示并先备份草稿**。
4. 展示状态提示：未保存 / XML 错误 / 引用错误 / 语义错误 / 可发布；错误不能在运行时被静默忽略。
5. schema `version` 发布后 immutable；若流程运行中协议被编辑，正在运行的实例继续使用当时的 compiled schema 快照；下一轮运行可选择新版。

## 6. 复制与引用（用户最高频操作）

- “复制字段”：深拷贝字段定义，生成新的 field UUID 和本层唯一 `key`（例 `deviceAddr_copy`），保留 label/长度/enum/Bit/默认值；对 `countRef` 等引用检查来源可达性。
- “复制整个结构/数组”：深拷贝所有子字段，内部引用以相对路径映射到新字段；外部引用如 `device.address` 不改变；有冲突必须提示。
- “引用共享片段”： `<use ref="muspHeader23"/>`，所有引用明确绑定片段版本，发布后不可变；更新共享片段须触发受影响信令校验。
- “另存模板”：新建独立模板文件/版本，支持跨协议库搜索并插入。
- “复制源码”：允许多行 XML 直接复制粘贴；若包含相同 key 自动提出 `重命名/替换/取消`，不允许默默覆盖原字段。
- “按示例生成 N 个字段”：允许从一个已有字段生成连续 N 个变体，自动维护 key、label 和可选 HEX 初始值；不是把 XML 手动粘贴一百次。

## 7. 静态校验器

发布前至少校验：XML 语法；schemaVersion 受支持；协议和信令码唯一；字段 key 唯一（作用域内）；引用不存在/循环引用；固定长度与子字段长度求和一致；位域重叠（保留位特殊处理）；数组最大 count、长度溢出；lengthExpr 的正向依赖；字段条件可达性；自动计算长度与 CRC 的范围；非法枚举和字段越界；XML DTD/ENTITY 拒绝。

**防脚本注入：** `when/lengthExpr` 使用 Rust 自建小表达式 parser 或受限表达式解释器，不可 `eval` JS，不可调用文件/网络/系统 API。支持 `bit(x,n)`、整数、基本四则/比较、字段引用与逻辑组合；限制运算步数/嵌套深度。

## 8. 解析视图规范

协议定义表固定五列且**无位置**；解码输出结构每字段需包含：`path`、`startOffset0`、`byteStart1`、`byteEnd1`、`bitStart`、`byteLength`、`rawHex`、`decodedValue`、`decimal`（若可数值解释）、`binary`、`labelZh`、`descriptionZh`、`unit`、`diagnostics`；类型为数组/结构的节点有 `children`。HEX 选区点击可反向定位字段，字段点击可高亮字节与相关 Bit。

## 9. 测试样例

1. `configFlags=0x81` 输出 Bit7=1、Bit0=1、DEC129。
2. `05 × 0A` 的 5 个 items 全部展开，items 每个长度10B，合计50B；5→3 时 bodyLength 自动缩小20B。
3. 设备配置 `0x22` 的 `05 AA AA AA AA AA` 返回全部成功，且不出现“失败列表”等不存在的字段。
4. XML 复制整个结构后，独立字段 key 无冲突、长度依赖仍正确。
5. 结构修改后编码/解码等价（golden vectors），错误 XML 不可发布。
6. `FF FE` 使用16位有符号解释成 -2、无符号解释成65534；展示 HEX 字节始终不变。



---

<!-- 来源文档: 02_协议与数据模型/04_流程引擎与业务场景.md -->

# 04 流程引擎、等待队列、设备/通话/资源状态

## 1. 流程模型

流程模板 FlowDefinition 是版本化有向图，Scenario 引用一个 FlowVersion、参与者集合、所使用协议版本和通信端点；每次执行产生新的 FlowRun。FlowRun 必须持有自己的 variables、pending requests、deadlines、设备参与绑定、日志 trace；同一个模板可以被 100 台设备并发使用。

节点：`START / END / SEND / WAIT_PACKET / SEND_AND_WAIT / CONDITION / ASSIGN / DELAY / LOOP / PARALLEL / JOIN / INVOKE_SUBFLOW / UPDATE_DEVICE / UPDATE_SESSION / UPDATE_CALL / UPDATE_RESOURCE / RAISE_EVENT / ERROR`。节点数据类型由 JSON schema/TS type 定义，但每个 SEND 节点只存协议+信令模板引用、字段绑定，而非写死原始 HEX。

触发：手动、场景自动、间隔定时、收到匹配信令、设备状态变化、上下文事件。流程必须可取消、可暂停或单步测试；暂停的含义必须清晰（暂停流程逻辑不等于停止网络收包）。

## 2. 一次请求响应的正确生命周期

```
FLOW_READY
  → build packet using published schema & context
  → register PendingRequest BEFORE UDP send
  → send UDP
  → WAITING
  ├─ valid reply (expectedMsgCode + correlation) → MATCHED → consume result → branch
  ├─ timeout → retry policy (same txn by default) → WAITING
  ├─ error reply → business negative branch
  └─ cancel/stop → CANCELLED
```

Waiting 不是线程阻塞；内存结构建议 `HashMap<CorrelationKey, Vec<PendingId>>` + 定时小根堆/DelayQueue，所有 pending 有唯一 ID 与防重复完成 CAS 状态。

结构建议：

```
PendingRequest {
 runId, flowRunId, waitNodeId, endpointId,
 protocolVersionId, requestMsgCode, expectedMsgCodes[],
 logicalDeviceId, expectSrcAddr3, expectDstAddr3,
 expectedRemoteIPPort, transactionId4, sessionMatcher,
 optionalCallId, optionalResourceId, optionalDirection,
 timeoutMs, retriesRemaining, deadline, status, sendAttemptIds[]
}
```

匹配步骤：本地端点→协议识别→HEX/CRC/长度→响应 msgCode 预筛→设备地址与对端→事务号→按会话规则→业务字段关联→唯一 pending。多个都匹配要报冲突并不唤醒，不能任意消费队列头。注册许可允许会话 0000→控制端分配新 sessionID 的差异化规则。

协议无序号时允许按 msgCode+设备/对端匹配；如果同一关联键下有两个无法区分请求，应拒绝并行或串行排队，不能保证 UDP 顺序天然正确。超时后迟到的包保留日志、不得重新唤醒已关闭流程。UDP 发送成功不等于业务执行成功。

## 3. 时间与重试

每个节点分别设 timeoutMs、retryCount（额外重试次数）、retryIntervalMs、sequenceReusePolicy、onTimeout branch、onMalformedPacket branch、onNegativeResponse branch；固定间隔和指数退避可选。

- 首次发送前建立 pending 防止快速响应漏匹配。
- 如果重试命令会影响外部业务，默认复用同一 transactionId，交由接收端幂等；可按真实协议覆写。
- 收到响应和超时同一时刻触发必须以原子状态先胜为准，另一方只记录迟到/被取消。
- 总流程 maxDuration、每节点 maxRetries、全局 pending 上限必须可配置防止无限循环。

## 4. 变量模型

System→Device→Session→FlowRun→Step；引用例如 `${device.address('MUSP')}` `${device.config.channels[0]}` `${session.authToken}` `${flow.txnId}` `${response.body.itemCount}`。由小表达式编译器在执行前检查字段/作用域；运行中按快照或最新设备配置由节点属性选择。

认证下发 `0x11` 提取 Token 到 `(deviceId, remoteSystemId, sessionId)` 的 session；后续消息通过 schema 字段绑定在正确位置自动注入；Token 过期走 `REAUTH_REQUIRED`，不能默默使用旧 Token。日志默认脱敏。

## 5. 设备三态（业务明确确认）

`OFFLINE=0x00`，`ONLINE=0x01`，`COMMUNICATING=0x02`。新创建设备 OFFLINE；收到注册完成成功 → ONLINE；某条通话按协议进入 ACTIVE → COMMUNICATING；结束最后一条有效通话且注册仍有效 → ONLINE；设备主动 `0x7D` 或心跳缺失达到判定条件 → OFFLINE。

“注册中”“等待配置”“资源撤销中”“心跳观察中”**都不是设备状态**。已有多通话时结束一条不会降级；设备 OFFLINE 要触发关联通话的结束子流程，但清理资源是否成功独立记录。

## 6. 设备配置场景

- `CONFIG_SET=0x21` 请求可选 8B configCommon，随后 10B prefix + items[count]，如5×10。
- 映射模式01：每个 item 对应系统下一个设备；模式02：所有 item 属于一个设备下的多组参数；映射必须按协议规则选择，不能仅凭 count 推断。
- `CONFIG_RESULT=0x22` 专用 body `05 AA AA AA AA AA`；数组索引与请求 items 对应，非 AA 只显示“未确认成功”，不造失败码含义。
- 配置提交策略：全量默认全部 AA 才提交目标快照；增量可只提交 AA 项。配置业务生效时间需可设置接收时、响应发送时、成功 ACK 时、指定事件时；不能单靠本地发送成功更新远端状态。
- 包含原子性 bit/结束标识的多段批次：请求可以跨多个 transactionId，按候选 batchId 聚合；Bit0=1 只是最后一段，不等于全部成功；实际远端是否支持事务回滚由真实协议决定。

## 7. 视频/语音通信流程

### A→B 视频

1. A `0x60 VIDEO_CALL_REQ`，带 B 的 3B 地址和希望的 RX/TX 带宽。
2. C（控制系统）向 B `0x62 PAGING_REQ` 等待 `0x63 PAGING_RSP`，允许拒绝/忙/超时分支。
3. 控制系统向 A `0x61 VIDEO_CALL_RSP`，向 B `0x64 CALL_NOTIFY`。
4. 控制系统按最终资源分配结果向 A 下发 `0x70 RX` / `0x71 TX`，向 B 下发 `0x70 RX` / `0x71 TX`；四个配置请求独立等 `0x72 ACK`。
5. LOCAL_READY：每台设备本地必要方向应用成功即可启用本机通信；CONTROL_NOTIFY：全部必要资源成功后等待控制系统下发 `0x75`，双方再启用。
6. 设备状态依据真实成功建立的通信业务计算为 COMMUNICATING。

### A→控制系统单端语音

1. A `0x65 VOICE_CALL_REQ` 携带希望的 RX/TX 带宽。
2. 控制端 `0x66 VOICE_CALL_RSP`；没有对无人机 B 的寻呼。
3. 控制端分别向 A 下发 `0x70 RX`、`0x71 TX`，每个独立等 `0x72`。
4. 按 LOCAL_READY 或 CONTROL_NOTIFY 规则启动。此处控制侧媒体承载资源仍应纳入控制资源池模型。

申请带宽、预留带宽、实际分配带宽要单独存；不能把申请数值作为资源下发的“实际成功带宽”。

## 8. 通话结束闭环

可触发：A/B `0x73 CALL_RELEASE_REQ`、控制系统主动结束、心跳失联、收到主动离线 `0x7D`、取消未建立通话 `0x7B`。

通用子流程：

```
trigger -> identify callId + participants
        -> 0x74 acceptance ONLY if there was 0x73
        -> notify reachable participants 0x78 (+ optional 0x79)
        -> stop this call's business media state
        -> for each participant/resource/direction:
              0x76 RELEASE_CFG -> wait 0x77 RELEASE_ACK
        -> aggregate success/unknown outcomes
        -> 0x7A CALL_END_RESULT (where applicable)
        -> device state recompute from remaining ACTIVE calls
```

A-B 视频典型四个资源（A.RX,A.TX,B.RX,B.TX）；单端语音典型两个（A.RX,A.TX）。如果某个离线设备不可达，**不需要向断电设备伪造成功 ACK**，其资源保持 UNKNOWN 不回池；可达参与方正常撤销，并依据是否还有其他通话恢复 ONLINE。

重复释放和两端同时挂断需要对 `(callId, runEpoch)` 加互斥/幂等：同一通话仅执行一次资源核销；不同发起者可分别获得自己的受理响应。

## 9. 通话及资源状态（与设备三态不同）

通话实例：`REQUESTED / PAGING / PERMITTED / ALLOCATING / READY / ACTIVE / ENDING / RELEASING / ENDED / FAILED / CANCELLED / CLEANUP_PENDING`。资源实例：`RESERVED / CONFIGURING / APPLIED / ACTIVE / RELEASING / RELEASED / UNKNOWN / FAILED`。这些是**通话/资源内部状态**，并不违背设备只有三态。

`ResourceKey = (projectId, scenarioRunId, callId, deviceId, resourceId, direction, version)`，防止 callId 2B 回绕复用时错误关联旧 ACK。释放容量只在收到有效的、与资源版本/方向一致的 `0x77` 之后可核销；如果响应超时为 UNKNOWN，不假定释放。

## 10. 必需工作流 JSON 基本形态

`06_示例与测试数据/register-flow.example.json` 给出示例流程序列化形态，包含节点/边、变量、协议引用、匹配规则、超时重试；格式可在 Codex 实现时作为第一版 schema。FlowDefinition 应验证 node ID、边目标、循环上限、子流程引用、协议版本固定、错误出口存在，才允许发布。

## 11. 最少必须运行的业务组合

- A/B 交换乱序注册响应 + 两个并发 0x02，事务/设备地址匹配无串包。
- 设备收到认证下发但注册流程正在等待许可，异步事件不误唤醒。
- `0x21` 的5条配置中有一条非 AA，准确追踪对应索引，不虚构失败原因。
- A-B 视频四条资源中 B.TX 超时，不能置通话 ACTIVE，已分配的资源按补偿清理。
- A 同时有视频+语音，关闭视频后状态继续 COMMUNICATING；最后一条结束后 ONLINE。
- A 掉线时 A→OFFLINE，B 收结束通知后若无其他业务则 ONLINE，A UNKNOWN 资源不回池。
- 两侧同时请求结束，资源池只发生一次真实资源核销。



---

<!-- 来源文档: 02_协议与数据模型/05_本地数据模型与持久化.md -->

# 05 本地数据模型、SQLite 与文件存储

## 1. 设计约束

- 数据保存在单机 SQLite；不需要安装 MySQL/MariaDB/Redis。
- XML 协议/工作流文件是可移植用户资产，SQLite 保存索引、发布版快照、运行记录与设备/会话信息；支持项目 ZIP 双向迁移。
- 每个实体用内部 UUID 主键，不能把 `00 00 3B` 当全局唯一 ID；相同协议地址可在不同系统/协议身份出现。
- 原始报文可大量产生：热日志 SQLite 元数据 + 可选块文件/压缩日志，按大小/天数滚动；不可无限增长。
- 数据库每次升级都有迁移和可回滚备份；WAL 模式，批量短事务写入。

## 2. 核心表目录（字段省略时由 Codex 具体设计）

| 表 | 主要字段 | 核心规则 |
|---|---|---|
| projects | id, name, created_at, data_path | 项目隔离 |
| systems | id, project_id, name, description | 一个项目多个系统 |
| protocols | id, name, identity, created_at | 协议逻辑概念 |
| protocol_versions | id, protocol_id, version, xml_source, compiled_checksum, published, created_at | 发布版不可改，草稿另存 |
| system_protocol_links | system_id, protocol_version_id | 系统↔协议多对多 |
| fragments | id, protocol_version_id, name, xml_path, content_hash | 公共头/字段组/Bit组可复用 |
| devices | id, system_id, display_name, type, state, origin, config_version, updated_at | state 只 00/01/02 |
| device_protocol_identities | id, device_id, protocol_id, address_bytes, route_hint, UNIQUE(system_id+protocol_id+address_bytes) 逻辑约束 | 地址按 bytes 存储，3B可显示HEX |
| device_config_snapshots | id, device_id, version, source_packet_id, json_blob, applied_at, status | 手动/报文变更历史与版本 |
| udp_endpoints | id, project_id, bind_ip, bind_port, local_role, remote_default, config_json | 端点与设备解耦 |
| scenarios | id, project_id, name, participants_json, flow_version_id, config_json | 场景配置 |
| flow_versions | id, flow_id, version, graph_json, protocol_lock_json, published | 发布版不可改 |
| flow_runs | id, scenario_id, flow_version_id, status, started_at, ended_at, failure_code | 运行历史 |
| pending_requests | id, flow_run_id, expected_codes, txn_id, src_addr, dst_addr, correlation_json, deadline, status | 内存运行时为准，必要信息落库追踪 |
| sessions | id, device_id, peer_system_id, wire_session_id, expires_at, state, secret_reference | 认证会话不与设备状态混同 |
| calls | id, scenario_run_id, wire_call_id, type, initiation_state, business_state, started_at, ended_at | callId 可复用，须作用域+运行纪元 |
| call_participants | id, call_id, device_id, role | 主叫/被叫/控制侧 |
| resources | id, call_id, participant_id, resource_id, direction, version, requested_bw, reserved_bw, assigned_bw, unit, state | RX/TX分别维护；UNKNOWN不回池 |
| packets | id, event_time, direction, endpoint_id, protocol_version_id, msg_code, flow_run_id, device_id, txn_id, byte_length, raw_ref, parse_status | 大量日志索引/分页 |
| packet_fields | packet_id, field_path, span_start, span_length, parsed_json | 按需缓存/重建字段树，不默认全量展开存储 |
| event_log | id, time, event_type, entity_id, metadata_json | 变更与异常审计 |

此目录为逻辑模型，不是已经验证的完整 SQL DDL。Codex 需实现 `migrations/0001_init.sql`、后续迁移及外键/索引和并发测试。

## 3. 必须维护的约束

- `devices.state` CHECK IN (`00`,`01`,`02`) 或以整数 `0/1/2` 明确映射；不可存 OFFLINING/REGISTERING。
- `device_protocol_identities.address_bytes` 是 blob/hex，MUSP 必须是3字节，其他协议可按对应 schema 不同长度。
- 设备变更来源 `MANUAL/PACKET` 保留来源 txnId/packetId；无法识别的报文不创建“幽灵设备”。
- `calls` 使用内部 UUID；网络的 `wire_call_id` 仅在 scenario/run/peer 范围内唯一；必须避免旧 callId 回复对上新通话。
- `resource` 的唯一归属包含 callId/设备/方向/资源ID/版本；逻辑或唯一索引禁止重复核销。
- `sessions` 关联设备 + 对端 + 协议会话号，Token 应加密存储或仅保持运行时，不可直接以无保护明文输出到日志/导出数据。
- packets 超过限制按明确保留策略滚动，场景中的日志 export 不在内存里一次展开全部记录。

## 4. 项目导入导出 ZIP 格式

```
project-export.zip
  manifest.json           # formatVersion, checksum, productVersion
  protocols/*.xml
  flows/*.json
  scenario/*.json
  devices/*.json          # 可选；敏感会话默认不导出
  fixtures/*.hex
  logs/*                  # 默认不包括，用户明确勾选才导出
```

ZIP 导入要防路径穿越 (`../`) 和 ZIP bomb；预览协议/设备与已有项目冲突后由用户选择新建项目、合并/跳过，不静默覆盖。

## 5. 历史数据与重启恢复

- 收到 CONFIG_RESULT 并通过匹配后才按配置生效策略更新设备快照；部分成功对应索引要保留，不得生成不存在的错误码中文说明。
- 进程异常结束：未完成 flow 变为 `INTERRUPTED`（流程内部状态），不改变已确认设备三态；存疑资源仍 UNKNOWN。
- 重启不自动重发控制指令；先由用户打开“恢复检查”并按协议定义的恢复流程查询设备状态。
- 可以将运行日志和设备历史导出，不默认导出 Token。



---

<!-- 来源文档: 03_开发与测试/06_技术架构与工程规范.md -->

# 06 技术架构与工程规范

## 1. 技术栈冻结

| 层次 | 选型 | 约束 |
|---|---|---|
| 桌面壳 | Tauri 2.x | 目标 Windows 10/11 x64，单机本地安装；使用 Tauri 官方插件/API |
| UI | React 19 + TypeScript 5.x + Vite | TypeScript strict 开启 |
| 组件 | Ant Design 5.x | 自定义暗/亮 theme tokens，禁止混入多套组件库 |
| 状态 | Zustand 5.x + TanStack Query 5.x | UI 临时状态与后端源数据分离 |
| 流程画布 | @xyflow/react 12.x | 节点与边数据完全序列化 |
| 代码编辑 | monaco-editor + @monaco-editor/react | XML/JSON、语法诊断与快捷操作 |
| 列表 | @tanstack/react-virtual | 收发日志、设备表大量行虚拟滚动 |
| Rust | Rust stable（锁定 rust-toolchain.toml） | edition 2024，Windows MSVC 工具链 |
| 并发 UDP | Tokio 1.x | async UdpSocket，取消令牌、队列背压、超时调度 |
| XML | quick-xml + serde/自定义 DSL 编译 | XML 禁 DTD/实体扩展，受限表达式与严格校验 |
| 本地存储 | SQLite + rusqlite 0.x 或 sqlx SQLite | **只选一种**，首期推荐 rusqlite；迁移脚本版本化、WAL |
| 错误与日志 | thiserror / anyhow / tracing | JSON structured log + 用户可读中文错误码 |
| 测试 | Vitest + RTL、cargo test、Playwright（UI） | Rust 编解码 golden vector tests 必需 |
| Windows 发行 | Tauri NSIS Setup.exe | 支持 offlineInstaller WebView2；非开发用户无 Node/Rust 依赖 |

依赖具体小版本不得猜测。创建工程时由包管理器选定同系列当前互相兼容的稳定小版本，`pnpm-lock.yaml`、`Cargo.lock` 和 `rust-toolchain.toml` **全部提交锁定**；Node 建议使用当前 LTS 并记录实际版本。开发统一 `pnpm`，禁止 npm/pnpm 混用。不要写“总是使用最新版本”让未来构建不可复现。

## 2. 架构分层

```
React UI / Monaco / React Flow / AntD
            │ typed Tauri invoke + event subscriptions
            ▼
Tauri IPC Commands + Validation + Authorization Boundary
            │
            ├─ Project/Schema Repository (SQLite + Files)
            ├─ XML Parser → Validated Schema AST → Compiler
            ├─ Binary Codec (Encode/Decode/HEX/CRC/Bit/Repeats)
            ├─ Protocol Router (endpoint + recognition + msgCode)
            ├─ Workflow Engine (run snapshots, wait-index, timers, events)
            ├─ Device/Session/Call/Resource Services
            └─ UDP Transport (Tokio sockets, rx/tx, bounded queues)
```

核心原则：**UI 不直接参与网络收发/报文编码/流程定时**；React 占用主线程不影响 UDP；所有状态先在 Rust 域服务里形成，再投递节流的 UI 事件；所有修改支持本地持久化。

## 3. 建议 monorepo 目录

```
musp-studio/
├── package.json
├── pnpm-lock.yaml
├── src/                            # React
│   ├── app/ routes/ layout/ theme/
│   ├── features/ workspace/ protocol/ device/ flow/
│   ├── features/ monitor/ call/ template/ settings/
│   ├── components/ HexGrid/ FieldTree/ BitEditor/ CodeEditor/
│   ├── api/tauri.ts                 # 唯一 IPC 包装层
│   ├── store/ hooks/ types/ utils/
│   └── tests/
├── src-tauri/
│   ├── src/main.rs
│   ├── src/lib.rs
│   ├── src/commands/
│   ├── src/domain/ models/ services/
│   ├── src/protocol/ xml/ ast/ compiler/ codec/ crc/ expressions/
│   ├── src/runtime/ router/ workflows/ pending/ timers/ sessions/
│   ├── src/network/ udp.rs
│   ├── src/persistence/ db.rs migrations/
│   ├── src/security/ settings/
│   └── tests/fixtures/
├── protocols/ musp/                # 样例，随发行包打包为只读种子
├── workflows/ samples/
├── docs/                            # 从本交付包迁入
├── scripts/ verify/ package/
└── .github/workflows/（可选）
```

## 4. 最重要的后端接口（示意，需生成 TS 类型）

| IPC 命令 | 入参 | 返回/事件 |
|---|---|---|
| `project_create/open/export` | 项目路径、名称 | projectId / ZIP 文件 |
| `protocol_validate` | XML string, protocolId | diagnostics（错误行列/字段路径）+ compile result |
| `protocol_publish` | protocolId, draftVersion | immutable publishedVersion |
| `protocol_encode` | publishedSchemaId, msgCode, context, fieldValues | Bytes + decoded field spans |
| `protocol_decode` | protocol hint/auto, ByteArray | PacketTree + diagnostics + rawBytes |
| `udp_endpoint_start/stop` | endpoint config | started/error + metrics |
| `udp_send` | endpointId, remote, bytes/encodedPacket | receipt（本地发送结果） |
| `workflow_run/stop` | scenarioId, flowVersion, devices | runId / event stream |
| `pending_list` | run/endpoint filters | waitItems, deadline, attempts |
| `device_upsert/list` | systemId, fields/mapping | device info/history |
| `call_list/resources` | scenario/callId | call/resource snapshots |
| `packet_query/export` | time/device/msgCode/direction | paginated rows/export path |

IPC 内禁止一次性传输十万条日志或不受限制的巨大 byte 数组；分页和事件增量需受控，原始报文必要时按 packetId 查询。

## 5. 接收通路细节

1. 一个 endpoint 的 socket 统一接收 UDP，无论有多少流程在监听，都不重复 bind 同一 IP:port。
2. 成功收包后先附加真实接收时间、原始源 IP:port、localEndpointId；在后台进行协议识别；识别规则可使用 endpoint/源地址/magic/headerVersion 等。
3. 选择唯一发布版 decoder；长度和 CRC 校验按配置规则处理，错误包也保留原始数据和失败诊断，不直接丢失。
4. 路由器依据 `(schemaVersion, msgCode, endpoint)` 定位匹配实例，再按 `(设备身份, src/dst, transactionId, sessionId, remoteEndpoint)` 匹配 PendingRequest；条件都满足且只匹配唯一待响应项才完成唤醒。
5. 不存在 pending 时交给异步触发器（例如 0x11 AUTH_ISSUE、心跳、配置主动下发），未匹配进入独立记录。
6. 匹配、取消 timeout、更新流程状态必须原子化；任何重复 ACK、迟到包不得重复结算。
7. UI 事件批量传输：例如 100–250 ms 合并日志变更/状态统计，事件过载时记录落盘队列溢出计数。

**顺序说明：** 单 socket 接收可在本进程中按读出顺序入队，但 UDP 网络、发送方及并发处理不保证严格业务顺序；需依据协议序号和事务号判断，绝对不承诺仅靠接收时间恢复真实发送顺序。

## 6. 资源线程与防止卡死

- Tokio task: endpoint receive/write、decoder worker pool（有界）、scheduler、workflow actors；SQLite 写入使用专门同步队列或 `spawn_blocking`，不能在异步 reactor 上长时间锁库。
- 设备及 call 的并发写操作按内部唯一 ID 做排他或 CAS 版本号；外部入包多个分支不得并发覆盖同一 session/token 状态。
- 设定可配置上限：最大单包长度、最大动态数组条目、最大嵌套层级、每场景最大设备数、最大并发 pending、每条 log bytes、总日志磁盘空间。
- 支持场景取消：stop 会取消等待定时器与循环发送，关闭或复用 socket，保证 UI 不出现假运行状态。
- 避免 Mutex 跨 `await`；持久化事务应短而明确。

## 7. 本地数据路径与备份

- 用 `app_data_dir()` 获取稳定目录：`projects/`、`db/musp.db`、`logs/`、`export/`、`settings.json`。
- 内置协议种子随应用发行，只读；首次创建项目复制或引用种子，用户修改生成新版本，不覆盖内置示例。
- 关闭 app 时自动停止 endpoint；重新打开不自动重发写操作/资源释放指令，要求用户显式恢复或按场景恢复策略执行。
- 数据库应用内迁移；定期/手动备份 SQLite 和 XML/流程文件，项目导出 ZIP 带 manifest+版本+校验值，导入预览差异。

## 8. 安全

- Tauri capability 配置最小权限：仅允许明确的文件对话框、app 数据目录读写和特定命令；用户导入 XML 不得读取任意本地路径（包括外部实体/DTD）。
- XML/表达式不可执行 JS/Rust/PowerShell 任意代码，使用明确允许的运算集合 `+ - * /`, 比较、逻辑、bit、len、slice、CRC；操作符有限制深度与时间预算。
- 自定义配置可包含敏感 Token：日志默认掩码，原始 UDP 可能携带明文 Token，应明确可选择屏蔽/禁存，并在文件导出时二次检查。
- 不默默开放公网监听：bind 0.0.0.0 时给予提示；对不可信远端绝对不能把报文长度当分配内存指令。

## 9. 构建验收

- `pnpm install --frozen-lockfile && pnpm build && pnpm test`
- `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
- `pnpm tauri build --bundles nsis`（最终 Windows 实际构建命令以创建的 Tauri 2 项目 CLI 版本核对）。
- Windows 10/11 x64 实机安装、卸载、新机器离线安装和回归测试；无 Java/Node/Redis/MySQL 部署步骤。



---

<!-- 来源文档: 03_开发与测试/07_测试计划与验收标准.md -->

# 07 测试计划、质量门槛与交付验收

## 1. 测试层次

1. Rust 单元：XML Parser/AST、字段长度、Big/Little Endian、bitfield、CRC、数组、条件、枚举、签名、expression 边界。
2. Rust 集成：本机两个 UDP 端点模拟 A 与控制系统，收发往返、多个设备并发请求等待和超时重试。
3. 领域服务：设备状态、配置应用规则、会话信息、call/resource 状态转换和幂等回收。
4. UI 单测与 E2E：字段复制、XML 在线编辑、报文选择反向定位、流程拖拽与节点编辑、设备列表与状态筛选。
5. Windows EXE 验收：全新 x64 Windows、离线安装、启动、UDP Loopback、卸载残留行为确认。

## 2. 协议层逐条用例（基础）

| 用例 ID | 动作 | 期待结果 |
|---|---|---|
| P-001 | 基础公共头编码 | 23B；msgCode=1B；src/dst地址各3B；session=2B |
| P-002 | 对 0x21 开启 configCommon8、count05 itemSize0A | bodyLength=68B；数组5×10B；报文生成后CRC正常 |
| P-003 | 0x21 关闭 configCommon8 | bodyLength=60B；与历史样本兼容 |
| P-004 | 0x22 输入 `05 AA AA AA AA AA` | 字段数量 5、结果逐项 AA、不添加额外响应字段 |
| P-005 | 0x22 count=05但只跟4字节 | 长度错误，不当作成功配置 |
| P-006 | Bit 控制 `81` | BIN10000001 / DEC129 / Bit7=1 Bit0=1 |
| P-007 | Bit 控制 `80` | Bit7=1 Bit0=0；解析文案正确 |
| P-008 | 有符号两字节 `FF FE` | -2；无符号解释65534，HEX不变 |
| P-009 | 不存在的协议/信令码 | 原始HEX照常可看，“协议未知/信令未知”警告 |
| P-010 | 故意修改一个 CRC 字节 | 校验失败；不应用设备配置 |
| P-011 | 动态数组 count 超上限 | 有界错误，无OOM与越界 |
| P-012 | 改 itemSize 与子结构长度不符 | 编译前报错，不偷偷填充 |
| P-013 | 深复制嵌套数组结构 | 独立 key/id、内部引用正确映射 |
| P-014 | XML 添加非法标签、DTD/ENTITY | 拒绝发布，有行列号并保留草稿 |
| P-015 | XML 发布后编辑该协议 | 正在运行 flow 仍使用旧版本快照 |

## 3. UDP/流程集成用例

| 用例 ID | 动作 | 期待结果 |
|---|---|---|
| W-001 | A、B 同时申请注册，交换响应到达顺序 | 仅唤醒对应事务/设备 pending |
| W-002 | 应答 msgCode 正确但 txn 错 | 不匹配，pending 继续等待直到正确/超时 |
| W-003 | 应答比 UI 刷新更快到达 | 因 pending 在发送前创建，能正确匹配 |
| W-004 | 等待注册许可时收到 0x11 认证下发 | 异步流程处理，主等待项不被误唤醒 |
| W-005 | 超时边界重复 ACK、慢 ACK | 只结束一次 pending，不重复应用 |
| W-006 | 真实协议无 txn 的相同并行请求 | 明确拒绝/串行或提示匹配冲突，不能随机匹配 |
| W-007 | UDP 发送失败（无本地绑定/非法对端） | 清晰失败状态且清理 pending |
| W-008 | 同端点多个监听流程 | 仅一个 socket bind，统一分发 |
| W-009 | 关闭场景时仍有循环/等待 | 取消，停止发送，不再输出虚假进度 |

## 4. 设备/通话/资源用例

| 用例 ID | 场景 | 期待结果 |
|---|---|---|
| D-001 | 手动新增设备 | 默认OFFLINE=00 |
| D-002 | 0x21 mode01 返回5个AA | 对应5台设备创建或更新，默认离线 |
| D-003 | 0x21 mode02 返回5个AA | 同一设备更新5组参数，不创建5台新设备 |
| D-004 | 已注册设备修改配置 | 不擅自从ONLINE重置为OFFLINE |
| D-005 | 注册完成0x04成功 | ONLINE=01 |
| D-006 | 视频四项资源都成功且启用 | 两端 COMMUNICATING=02 |
| D-007 | 视频资源有一项失败 | 不显示通话整体ACTIVE，执行补偿策略 |
| D-008 | A同时视频+语音，结束视频 | A仍 COMMUNICATING |
| D-009 | A结束最后一条通话 | A变ONLINE，只影响该call资源 |
| D-010 | A突然掉线，B仍在线 | A OFFLINE，B按余下通话恢复ONLINE，A UNKNOWN资源不回池 |
| D-011 | 两方同时挂断同一call | 一轮资源撤销，无二次核销 |
| D-012 | 重试重复同一0x76 | 幂等，无重复释放计数 |
| D-013 | 0x7D主动离线 | 状态OFFLINE，清理流程和设备实例保留 |
| D-014 | 设备已离线时重复心跳断线判定 | 不创造新设备状态，不重复清理同一call |

## 5. 用户界面 E2E 用例

| 用例 | 验收标准 |
|---|---|
| 复制字段 | Ctrl+D 复制字段后修改新字段不影响原字段；有效XML/AST一致 |
| 共享片段 | 多信令可引用同一公共头；更改并发布新版后受影响模板校验，运行中不变 |
| XML 双向编辑 | 改 XML 后可视化正确；改可视化后源码正确；非法 XML 不丢草稿 |
| 位域编辑 | `81` 勾选 Bit0→0 生成 `80`，HEX→BIT 反向刷新 |
| HEX 双向定位 | 点击 Byte 30 高亮中文字段及 Bit，点击字段高亮HEX范围 |
| 解析表 | 显示字节位置和HEX/DEC/中文；协议主表没有偏移列 |
| 多协议/设备 | 相同地址值在不同系统不混淆；信令选择设备自动引用对应协议地址 |
| 多端点 | 端点各自启动/停止，异常端口占用反馈清晰 |
| 深色切换 | 单击即时切换，不要求用户再次确认；持久化 |
| 大量日志 | 滚动/过滤不阻塞 UDP 和整个桌面应用 |

## 6. 性能验收方法（可调整测试硬件，不应空口保证）

基准机先指定 16GB RAM、现代 6 核 CPU、Windows 11 x64、回环 IP、本地日志限速开启；记录实际测试机规格。目标**首次工程验收**：

- 1,000 UDP 数据报/秒连续 5 分钟（≤512B），协议解析、待响应索引及日志入库稳定，进程不死锁、不显著内存递增；严格区分发送量、内核收包数、解析量、队列丢弃量。内核网络丢包不做不可能的零丢包承诺。
- 一分钟 burst 5,000 包/秒，系统依然响应；队列满会记录准确可见的 drop count，不悄悄丢弃。
- 100 个设备实例并发注册及两个独立控制端，事务关联零误匹配（由确定性测试驱动注入）。
- 同一报文的 HEX ↔ 解析树响应 UI 交互在正常数据量下不明显卡顿；所有日志列表必须虚拟化。
- 1,000 条动态数组条目只有在 schema 上限允许时测试，超过上限必须可控拒绝。

这些是**待 Codex 实测的验收目标，不是已达成指标**。

## 7. 发布检查

- 所有 P0 核心流程 E2E 完成；P1 列表明确标“已完成/未完成”，不因看起来像原型就宣称可用。
- `cargo test` + TS tests + lint/typecheck + XML golden vector 全绿。
- Windows NSIS Setup.exe 可在干净虚拟机安装、卸载；离线 installer 不依赖外网下载安装 WebView2；适当检查本地防火墙授权提示。
- 完成 README 用户手册、示例协议、5×10配置+AA响应 sample、版本号、变更记录、故障排查。
- 安全检查：XML 无 DTD/XXE、表达式无代码执行、日志 Token 脱敏、项目导入防路径穿越。
- 未定义的结果码保持 HEX+“未知”，不得给出自造失败中文描述。



---

<!-- 来源文档: 03_开发与测试/08_迭代计划与Windows安装包.md -->

# 08 开发阶段与 Windows 安装交付

## 1. 实施策略

**先底层协议和运行引擎，再集成 UI**，避免做出功能按钮齐全却不能解析真实 UDP 数据的“演示后台”。Codex 可以一次接受主指令，但必须按阶段推进并每阶段提供实际测试输出与已完成/未完成清单。

| 阶段 | 目标 | 可运行交付与退出条件 |
|---|---|---|
| S0 | 建项目骨架 | Tauri2+React TS+AntD，Windows dev 可启动，SQLite migration，主题+导航+项目基础 |
| S1 | XML Schema & Codec | XML 可编辑、复制结构、校验发布；Rust encode/decode 与 CRC/Bit/动态数组 golden tests |
| S2 | UDP 与报文工作台 | 多端点、UDP tx/rx、HEX/字段树/中文解释/包内定位、分页日志，可本地双进程回环验证 |
| S3 | 设备与会话 | 手工建设备/协议字段绑定/0x21配置/0x22逐条AA/0x01–04注册/Token变量传递 |
| S4 | 可视化流程引擎 | @xyflow/react 节点、发布/运行、PendingRequest匹配、超时重试、并行/分支/子流程 |
| S5 | 通话与资源 | 0x60-0x7D样例，视频/语音/带宽RX/TX/结束/离线/UNKNOWN资源回收 |
| S6 | 产品完善与打包 | 模板库导入导出、故障注入、报告、E2E、Windows NSIS/离线安装、用户说明书 |

建议首个里程碑 **S0+S1+S2** 即可真实创建 XML 协议并本机 UDP 发包/收包/解析；此时虽还不具备全部业务，也必须已经有可运行原型和测试，而不是静态图片。

## 2. Windows 环境约束

- 使用 Windows 10/11 x64 原生 MSVC Toolchain 完成最终 Tauri 构建；安装 Rust、Microsoft C++ Build Tools（Desktop Development with C++）、WebView2（开发机）、Node LTS、pnpm。
- CLI 的精确命令和 JSON 配置选项以 Tauri 2 当前官方文档核对；`pnpm tauri build --bundles nsis` 作为计划命令，实际 package.json 中固定 scripts。
- 安装包 `MUSP-Studio_<version>_x64-setup.exe`；用户安装软件不需要 Node/Rust/Java/数据库服务器。
- 软件首次运行创建 SQLite 文件与项目目录，在 `%APPDATA%` 或 Tauri app data 目录；卸载程序时用户的项目文件不应默认静默删除。
- 在 Windows WebView2 未安装环境，官方 Tauri NSIS 支持 `webviewInstallMode: offlineInstaller` 生成可离线安装包；离线包相对较大，验收前确认包内依赖及许可合规。

## 3. Tauri 配置示意（需按真实 Tauri 2 配置验证）

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "MUSP Studio",
  "version": "0.1.0",
  "identifier": "dev.musp.studio",
  "build": {
    "frontendDist": "../dist",
    "devUrl": "http://localhost:5173"
  },
  "bundle": {
    "active": true,
    "targets": ["nsis"],
    "windows": {
      "webviewInstallMode": { "type": "offlineInstaller" }
    }
  }
}
```

**说明：** 该片段是待在真实工程中验证的计划配置。不能把仅生成的 JS 代码或 `cargo build` 二进制当成已经交付的 Windows 安装 EXE。

## 4. 最终用户安装体验

1. 双击 `MUSP-Studio_*_x64-setup.exe` → 安装向导 → 安装完成。
2. 双击桌面快捷方式 → 首次启动选择“新建项目 / 打开现有项目 / 加载演示项目”。
3. 演示项目预置 MUSP v0.8 业务协议、注册/配置/视频/语音/释放示例流程、两套虚拟通信对端。
4. 默认不自动绑定公网端口、不自动连接任何外部设备；用户进入场景明确点击“启动”。
5. XML 在线编辑器可用、报文解析可用、SQLite 自动初始化。
6. 支持“设置→备份导出”保存项目；关闭软件主动停止所有本地 UDP socket。

## 5. 版本与交付目录

```
release/
  MUSP-Studio_0.1.0_x64-setup.exe
  MUSP-Studio_0.1.0_checksums.sha256
  README_安装使用.md
  CHANGELOG.md
  THIRD_PARTY_NOTICES.md
  test-report.html
```

安装签名不是本次设计包内已有能力：正式外部分发建议配置 Windows 代码签名，未签名安装程序可能触发 SmartScreen 提示，不能在文档中保证消除。

## 6. 完成定义（Definition of Done）

- 关键功能存在**可运行实现**、自动化测试与人工 E2E 证据，按钮动作真实可用。
- 每个阶段有 Git 提交/变更记录与测试结果；没有实现的功能要标注为未实现，不能使用假数据冒充真实设备运行结果。
- 在新机器上完成离线安装和本地UDP回环协议测试，最后产物为真正的 NSIS 安装 EXE。
- 对原始 MUSP v0.8 仍存在“待确认”内容维持配置能力而不擅自定义其行业语义。

