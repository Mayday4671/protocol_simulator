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
