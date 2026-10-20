//! 核心逻辑库：数据模型、Markdown 协议解析、模板渲染、字段级定义、规则匹配。
//! 不依赖 Tauri/GUI, 可独立运行单元测试。

pub mod error;
pub mod datagram;
pub mod fields;
pub mod headers;
pub mod xml;
pub mod models;
pub mod profile;
pub mod render;
pub mod rules;
pub mod template;
pub mod traffic_stats;
