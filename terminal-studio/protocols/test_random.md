# 随机长度测试协议集（字段定义模式）

用于测试 20~1400 字节变长报文的发送与解析展示。
「数据域」字段值填 `{rlen:最小-最大}`, 每次发送随机取长、随机内容;
解析时该字段自动吃掉报文剩余全部字节, 长度随实际报文显示。
导入方式: 「协议管理」→「导入 .md」选择本文件。

## 协议名称: 短报文测试
- **ProtocolID**: test_short
- **Type**: Event
- **System**: 演示系统
- **Group**: 设备
- **Description**: 帧头+命令+序号+数据域, 总长 24~64 字节随机

### Fields
| 字段   | 长度(B) | 编码 | 值          |
| ------ | ------- | ---- | ----------- |
| 帧头   | 2       | hex  | FF C1       |
| 命令字 | 1       | hex  | 01          |
| 序号   | 4       | uint | {seq}       |
| 数据域 | 64      | hex  | {rlen:16-56} |

### Parse Rules
- **Match**: `^FF C1`

## 协议名称: 中报文测试
- **ProtocolID**: test_medium
- **Type**: Event
- **System**: 演示系统
- **Group**: 设备
- **Description**: 总长 136~520 字节随机

### Fields
| 字段   | 长度(B) | 编码 | 值           |
| ------ | ------- | ---- | ------------ |
| 帧头   | 2       | hex  | FF C2        |
| 命令字 | 1       | hex  | 02           |
| 序号   | 4       | uint | {seq}        |
| 数据域 | 512     | hex  | {rlen:128-512} |

### Parse Rules
- **Match**: `^FF C2`

## 协议名称: 长报文测试
- **ProtocolID**: test_long
- **Type**: Event
- **System**: 演示系统
- **Group**: 设备
- **Description**: 总长 520~1400 字节随机, 验证大报文展示

### Fields
| 字段   | 长度(B) | 编码 | 值            |
| ------ | ------- | ---- | ------------- |
| 帧头   | 2       | hex  | FF C3         |
| 命令字 | 1       | hex  | 03            |
| 序号   | 4       | uint | {seq}         |
| 数据域 | 1392    | hex  | {rlen:512-1392} |

### Parse Rules
- **Match**: `^FF C3`

## 协议名称: 满包测试
- **ProtocolID**: test_max
- **Type**: Event
- **System**: 演示系统
- **Group**: 设备
- **Description**: 固定 1400 字节满包（UDP 上限附近）

### Fields
| 字段   | 长度(B) | 编码 | 值              |
| ------ | ------- | ---- | --------------- |
| 帧头   | 2       | hex  | FF C4           |
| 命令字 | 1       | hex  | 04              |
| 序号   | 4       | uint | {seq}           |
| 数据域 | 1392    | hex  | {rlen:1392-1392} |

### Parse Rules
- **Match**: `^FF C4`
