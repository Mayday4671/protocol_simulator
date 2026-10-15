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
