"""Build an offline release report only from passing evidence for this release."""

import hashlib
import html
import json
from pathlib import Path
import re
from urllib.parse import urlparse


CURRENT_EVIDENCE = {
    "install": "test-results/flow-workspace-install/report.json",
    "desktop": "test-results/flow-workspace-installed/report.json",
    "selftest": "test-results/flow-workspace-installed-selftest/self-test.json",
}
SCREENSHOTS = [
    ("flow-active-1440.png", "主动发送与响应：1440 像素窗口"),
    ("flow-active-1280.png", "主动发送与响应：1280 像素窗口"),
    ("flow-passive.png", "被动等待后按收到的字段回复"),
    ("flow-timeout.png", "等待超时与失败显示"),
]
HISTORICAL_EVIDENCE = [
    ("test-results/installation-final.json", "0.1.0 首次安装、升级、自测与卸载记录"),
    ("test-results/desktop-installed-final/report.json", "0.1.0 已安装版七页与功能巡检"),
    ("test-results/copy-cleanup-install.json", "0.1.0 文案更新后的安装与自测记录"),
]


def require(condition, message):
    if not condition:
        raise ValueError(message)


def read_json(path):
    value = json.loads(path.read_text(encoding="utf-8-sig"))
    require(isinstance(value, dict), f"Expected a JSON object: {path}")
    return value


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def checked_hash(value, label):
    require(isinstance(value, str) and re.fullmatch(r"[0-9a-fA-F]{64}", value),
            f"Missing or invalid {label}")
    return value.lower()


def checklist(items):
    require(isinstance(items, list) and items and all(isinstance(item, str) for item in items),
            "Passing evidence must include a nonempty checks list")
    return "<ul>" + "".join("<li>" + html.escape(item) + "</li>" for item in items) + "</ul>"


def evidence_details(title, value, source):
    return ("<details><summary>" + html.escape(title) + "</summary><p>原始记录：<code>"
            + html.escape(source) + "</code></p><pre>"
            + html.escape(json.dumps(value, ensure_ascii=False, indent=2)) + "</pre></details>")


def build_report(project_root):
    """Validate current evidence and return HTML; never write release files here."""
    project_root = Path(project_root)
    release = project_root / "release"
    version = read_json(project_root / "package.json").get("version")
    require(isinstance(version, str) and re.fullmatch(r"\d+\.\d+\.\d+(?:[-+][A-Za-z0-9.-]+)?", version),
            "package.json must contain a valid release version")
    evidence = {}
    for key, relative in CURRENT_EVIDENCE.items():
        value = read_json(project_root / relative)
        require(value.get("passed") is True, f"Current evidence did not pass: {relative}")
        evidence[key] = value
    install, desktop, selftest = (evidence[key] for key in ("install", "desktop", "selftest"))
    require(install.get("exitCode") == 0, "Current NSIS installation did not exit successfully")
    require(install.get("registrationRestored") is True, "Application registration was not restored")
    require(selftest.get("version") == version, "Installed self-test version differs from package.json")
    require(desktop.get("pageErrors") == [], "Installed desktop reported unhandled page errors")
    require(desktop.get("packetCount") == 4, "Expected the four actual active/passive UDP packets")
    url = desktop.get("url", "")
    require(isinstance(url, str), "Installed desktop evidence has no URL")
    location = urlparse(url)
    require((location.scheme in ("http", "https") and location.hostname == "tauri.localhost")
            or (location.scheme == "tauri" and location.hostname == "localhost"),
            "Desktop evidence must come from the installed Tauri application, not a dev server")

    installer_name = f"MUSP-Studio_{version}_x64-setup.exe"
    installer_hash = checked_hash(install.get("installerSha256"), "installerSha256")
    installed_hash = checked_hash(install.get("installedSha256"), "installedSha256")
    require(sha256(release / installer_name) == installer_hash,
            "Release installer does not match the tested installer SHA256")
    install_target = install.get("installTarget")
    require(isinstance(install_target, str) and install_target, "Installation evidence has no installTarget")
    require(sha256(Path(install_target) / "musp-studio.exe") == installed_hash,
            "Installed executable does not match the tested executable SHA256")
    for filename, _ in SCREENSHOTS:
        require((release / "screenshots" / filename).is_file(), f"Missing current screenshot: {filename}")

    title = html.escape(f"MUSP Studio {version} 验收报告")
    parts = ["""<!doctype html><html lang="zh-CN"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1"><title>""", title, """</title><style>
body{margin:0;background:#eef3f8;color:#203248;font:15px/1.8 'Microsoft YaHei UI',sans-serif}
main{max-width:1080px;margin:32px auto;padding:38px;background:white;border:1px solid #d6e0eb;border-radius:12px}
h1{font-size:28px}h2{border-top:1px solid #d6e0eb;padding-top:22px;margin-top:32px}
code{font:13px Consolas,monospace;background:#eef3f8;padding:3px 5px;overflow-wrap:anywhere}
.verified{padding:20px;background:#e7f7ef;border-left:4px solid #11834b}a{color:#075fc4}
details{padding:16px;margin:12px 0;background:#f5f8fb}img{max-width:100%;border:1px solid #d6e0eb;margin-top:12px}
pre{white-space:pre-wrap;overflow-wrap:anywhere;font:12px/1.6 Consolas,monospace}
</style></head><body><main><h1>""", title, """</h1>
<div class="verified"><strong>本次隔离安装、已安装 EXE 的真实 MUSP 自测与通用 UDP 流程验收通过</strong></div>
<p>当前结论仅来自以下三份本次记录，并核对安装包与已安装程序的 SHA256。旧版本安装、升级、七页巡检和卸载记录在文末历史区单独保留，不代表本版本重复执行。</p>
<p>本次对端使用真实本机 UDP socket，未连接外部硬件；全新无 WebView2 的离线虚拟机尚未验证。应用未配置代码签名，SHA256 用于完整性核验。</p>
<p>用户已取消高频压力验收。原失败或中止样本仍保留，不作为本次交付门槛，也不据此声称达到高吞吐指标。</p>
<h2>当前安装产物</h2><p>安装包：<code>""", html.escape(installer_name),
             "</code><br>安装包 SHA256：<code>", installer_hash,
             "</code><br>已安装 EXE SHA256：<code>", installed_hash,
             "</code><br>隔离安装后应用注册信息已恢复。</p>",
             evidence_details("本次隔离安装原始记录", install, CURRENT_EVIDENCE["install"]),
             "<h2>已安装程序的真实 MUSP 自测</h2>", checklist(selftest.get("checks")),
             evidence_details("本次安装版自测原始记录", selftest, CURRENT_EVIDENCE["selftest"]),
             "<h2>已安装程序的流程页真实 UDP 验收</h2>", checklist(desktop.get("checks")),
             "<p>实际页面来源：<code>", html.escape(url),
             "</code>；未处理脚本异常：0；主动与被动流程实际报文总数：4。</p>",
             evidence_details("本次流程页验收原始记录", desktop, CURRENT_EVIDENCE["desktop"])]
    for filename, caption in SCREENSHOTS:
        parts.append('<details><summary>' + html.escape(caption) + '</summary><img src="screenshots/'
                     + filename + '" alt="' + html.escape(caption, quote=True) + '"></details>')

    parts.append("<h2>历史证据</h2><details><summary>0.1.0 历史安装、七页巡检与卸载记录（不计入本次通过结论）</summary>")
    parts.append("<p>下列原始记录与截图属于此前的 0.1.0 交付及文案更新；保留其原始版本、文件名、散列和测试范围。</p>")
    for relative, label in HISTORICAL_EVIDENCE:
        path = project_root / relative
        if path.exists():
            parts.append(evidence_details(label, read_json(path), relative))
    for index, name in enumerate(["工作台", "协议设计", "设备管理", "交互流程", "报文监控", "通话资源", "模板库"], 1):
        filename = f"page-{index}.png"
        if (release / "screenshots" / filename).is_file():
            parts.append(f'<details><summary>0.1.0 历史截图：{name}</summary><img src="screenshots/{filename}" alt="0.1.0 历史{name}"></details>')
    parts.append('</details><p><a href="' + html.escape(installer_name, quote=True)
                 + '">当前 Windows 安装包</a> · <a href="SHA256SUMS.txt">SHA256 清单</a> · '
                 '<a href="README_安装使用.md">安装使用说明</a></p></main></body></html>')
    return "".join(parts)


def main():
    project_root = Path(__file__).resolve().parents[1]
    report = build_report(project_root)
    destination = project_root / "release" / "test-report.html"
    destination.write_text(report, encoding="utf-8", newline="\n")
    print(destination)


if __name__ == "__main__":
    main()
