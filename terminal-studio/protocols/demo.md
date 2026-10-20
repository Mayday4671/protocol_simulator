# 演示协议集：模拟终端 ↔ 目标系统

包含 入网请求 / 入网应答 / 心跳 / 状态上报 四个协议。
入网请求/应答采用**字段定义模式**（收发报文可逐字段解析），心跳类保留 Hex 模板模式。
导入方式: 「协议管理」→「导入 .md」选择本文件。

## 字典
- **System**: 演示系统

| 字典   | 名称     | 值 | 含义     |
| ------ | -------- | -- | -------- |
| cmd    | 命令字   | 01 | 入网     |
| cmd    | 命令字   | 02 | 心跳     |
| result | 入网结果 | 00 | 失败     |
| result | 入网结果 | 01 | 成功     |

## 协议名称: 入网请求
- **ProtocolID**: login_req_01
- **Type**: Request
- **System**: 演示系统
- **Group**: 设备
- **Description**: 终端向目标系统发起入网

### Fields
| 字段     | 长度(B) | 编码 | 值          | 字典 |
| -------- | ------- | ---- | ----------- | ---- |
| 帧头     | 2       | hex  | FF 00       | -    |
| 命令字   | 1       | hex  | 01          | cmd  |
| 终端编号 | 4       | uint | {node_id}   | -    |
| 时间戳   | 4       | uint | {timestamp} | -    |

### Parse Rules
- **Match**: `^FF 00 01`
- **AutoRespond**: `login_resp_01`
- **Delay**: 500

## 协议名称: 入网应答
- **ProtocolID**: login_resp_01
- **Type**: Response
- **System**: 演示系统
- **Group**: 设备
- **Description**: 目标系统对入网请求的应答, 01 表示成功

### Fields
| 字段     | 长度(B) | 编码 | 值    | 字典   |
| -------- | ------- | ---- | ----- | ------ |
| 帧头     | 2       | hex  | FF 81 | -      |
| 命令字   | 1       | hex  | 01    | cmd    |
| 结果     | 1       | hex  | 01    | result |

### Parse Rules
- **Match**: `^FF 81 01`

## 协议名称: 心跳
- **ProtocolID**: heartbeat_01
- **Format**: Hex
- **Type**: Heartbeat
- **System**: 演示系统
- **Group**: 设备
- **Description**: 入网成功后周期性发送的保活信令

### Payload Template
```hex
FF 02 {node_id} {timestamp} {seq}
```

### Parse Rules
- **Match**: `^FF 02`
- **AutoRespond**: `heartbeat_ack_01`
- **Delay**: 100

## 协议名称: 心跳应答
- **ProtocolID**: heartbeat_ack_01
- **Format**: Hex
- **Type**: Response
- **System**: 演示系统
- **Group**: 设备

### Payload Template
```hex
FF 82 01 {random:2}
```

### Parse Rules
- **Match**: `^FF 82`
