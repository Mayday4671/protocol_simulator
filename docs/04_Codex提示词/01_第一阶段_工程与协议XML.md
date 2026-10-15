# Codex S0+S1：先把协议引擎做实

以总控提示词及交付目录为约束，当前只聚焦**能运行的底层基础与 XML 编辑工作台**。建立 Tauri2+React19+TypeScript+AntD5+Rust stable/Tokio+SQLite 工程；配置 pnpm/Cargo 锁文件、SQLite 迁移与基础路由，七张原型图仅用于结构与视觉统一。

在 Rust 实现协议 XML parser、编译/校验器、AST、二进制 encode/decode、Bit/Struct/Array/条件字段/值解释/CRC16；在 React 使用 Monaco 编辑 XML、协议字段主表、字段属性侧栏，支持复制字段/数组组、拖动顺序、引用公共 fragment、保存草稿、发布版本。XML 作为源数据，发布版本 immutable，源码错误时保留草稿并阻止发布。

验收：先使示例 XML well-formed，然后实现 DSL 语义验证；golden-vectors.json 的 99B CONFIG_SET 与31B CONFIG_RESULT 必须编码解码一致；HEX81 Bit7/Bit0、5条10B、显示有符号/无符号DEC；不存在的引用、循环、危险 XML ENTITY 必须拒绝。请提供可执行测试和对应文件位置。不要把报文生成写在 React 里；不要只完成 UI。
