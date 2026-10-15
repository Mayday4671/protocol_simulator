# Codex S5+S6：视频/语音通信、资源池与 Windows EXE

按 v0.8 样例扩展 flow engine + domain services：A-B视频申请→寻呼→应答→通话响应/通知→A/B各自RX/TX资源配置等待0x72；A-控制语音申请→响应→A的RX/TX配置。申请带宽与最终分配带宽分别保存。支持 LOCAL_READY/CONTROL_NOTIFY 0x75 两种启用规则，按真实建立的ACTIVE通话更新设备三态。

完善 A/B/控制主动结束、0x73/74/78、0x76/77分项释放、0x7A结果、0x7D离线与心跳超时：分别维护业务终止与资源回收的状态，UNKNOWN 不回池，多 callId 不互相影响，两端同时挂断不二次释放。UI 是通话列表、详情、各参与设备RX/TX资源和异常记录，不要假按钮。

添加故障注入、日志过滤导出、项目XML+场景ZIP导入导出、模板库/复制结构，做 Vitest/cargo test/e2e 与 Windows 安装测试。配置 Tauri2 NSIS，WebView2 offlineInstaller；在 Windows 原生工具链上 `pnpm tauri build --bundles nsis` 生成真实可安装的 Setup.exe，给出路径、SHA256、已测试 Windows 版本与安装/卸载结论；环境不满足就给出正确的 Windows 构建工作流和实际完成边界，不得伪造 EXE 已成功。
