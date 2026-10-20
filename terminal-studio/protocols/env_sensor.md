# 环境监测传感器协议集 (ENV-SENSOR v1.1)

帧头 `FF A1`。注册类协议采用**字段定义模式**（可逐字段解析），心跳/告警保留 Hex 模板模式，两种方式可在同一文件混用。
导入方式: 「协议管理」→「导入 .md」选择本文件。

字典随本文件一起导入：字段表的「字典」列填字典 Key 即完成绑定，绑定后该字段的取值被限定为字典条目，
收发报文时会直接显示条目含义（如 `目标ID=01 (主站)`）。模板模式里用 `{dict:字典Key=含义}` 引用。

## 字典
- **System**: 物联网协议

| 字典      | 名称     | 值   | 含义       |
| --------- | -------- | ---- | ---------- |
| target_id | 目标ID   | 01   | 主站       |
| target_id | 目标ID   | 02   | 备站       |
| reg_addr  | 注册地址 | 0100 | 主站地址   |
| reg_addr  | 注册地址 | 0200 | 备站地址   |
| cmd       | 命令字   | 01   | 注册       |
| cmd       | 命令字   | 02   | 心跳       |
| cmd       | 命令字   | 03   | 告警       |

## 字典

| 字典   | 名称     | 值 | 含义 |
| ------ | -------- | -- | ---- |
| result | 应答结果 | 00 | 拒绝 |
| result | 应答结果 | 01 | 接受 |

## 协议名称: 传感器注册请求
- **ProtocolID**: sensor_reg_req
- **Type**: Request
- **System**: 物联网协议
- **Group**: 设备
- **Description**: 上电注册: 1B 目标ID + 2B 注册地址 + 4B 终端编号 + 4B 时间戳

### Fields
| 字段     | 长度(B) | 编码 | 值          | 字典      |
| -------- | ------- | ---- | ----------- | --------- |
| 帧头     | 2       | hex  | FF A1       | -         |
| 目标ID   | 1       | hex  | 01          | target_id |
| 注册地址 | 2       | uint | 0100        | reg_addr  |
| 终端编号 | 4       | uint | {node_id}   | -         |
| 时间戳   | 4       | uint | {timestamp} | -         |

### Parse Rules
- **Match**: `^FF A1 01`
- **AutoRespond**: sensor_reg_resp
- **Delay**: 200

## 协议名称: 传感器注册应答
- **ProtocolID**: sensor_reg_resp
- **Type**: Response
- **System**: 物联网协议
- **Group**: 设备
- **Description**: 平台受理结果, 2B 平台编号 + 6B 毫秒时间戳 + 1B 结果

### Fields
| 字段     | 长度(B) | 编码 | 值             | 字典   |
| -------- | ------- | ---- | -------------- | ------ |
| 帧头     | 3       | hex  | FF A1 81       | -      |
| 平台编号 | 2       | uint | 0x0001         | -      |
| 时间戳   | 6       | uint | {timestamp_ms} | -      |
| 结果     | 1       | hex  | 01             | result |

### Parse Rules
- **Match**: `^FF A1 81`

## 协议名称: 状态心跳上报
- **ProtocolID**: sensor_hb
- **Format**: Hex
- **Type**: Heartbeat
- **System**: 物联网协议
- **Group**: 设备
- **Description**: 注册成功后周期上报, 携带自增序号与 2 字节随机校验

### Payload Template
```hex
FF A1 {dict:cmd=心跳} {node_id} {seq} {random:2}
```

### Parse Rules
- **Match**: `^FF A1 02`
- **AutoRespond**: sensor_hb_ack
- **Delay**: 50

## 协议名称: 状态心跳确认
- **ProtocolID**: sensor_hb_ack
- **Format**: Hex
- **Type**: Response
- **System**: 物联网协议
- **Group**: 设备

### Payload Template
```hex
FF A1 82 {timestamp} 00
```

### Parse Rules
- **Match**: `^FF A1 82`

## 协议名称: 温湿度告警上报
- **ProtocolID**: sensor_alert
- **Format**: Hex
- **Type**: Event
- **System**: 物联网协议
- **Group**: 网关
- **Description**: 越限告警: 温度 0x1A (26℃), 湿度 0x46 (70%)

### Payload Template
```hex
FF A1 03 {node_id} 1A 46 {random:2}
```

### Parse Rules
- **Match**: `^FF A1 03`
- **AutoRespond**: sensor_alert_ack
- **Delay**: 100

## 协议名称: 告警确认
- **ProtocolID**: sensor_alert_ack
- **Format**: Hex
- **Type**: Response
- **System**: 物联网协议
- **Group**: 网关

### Payload Template
```hex
FF A1 83 {seq} 00
```

### Parse Rules
- **Match**: `^FF A1 83`
