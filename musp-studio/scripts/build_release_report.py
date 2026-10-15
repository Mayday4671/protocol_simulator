"""Render only verified local evidence into the distributable offline report."""
import html
import json
from pathlib import Path
import re

root=Path(__file__).resolve().parents[1]
release=root/"release"
install=json.loads((root/"test-results/installation-final.json").read_text(encoding="utf-8-sig"))
desktop=json.loads((root/"test-results/desktop-installed-final/report.json").read_text(encoding="utf-8-sig"))
assert install["passed"] and desktop["passed"],"Do not publish a passing report without passing evidence"
source=(root.parent/"docs/TEST_REPORT.md").read_text(encoding="utf-8-sig")

def inline(text):
    return re.sub(r"`([^`]+)`",r"<code>\1</code>",html.escape(text))

content=[]
table=False
for line in source.splitlines():
    if line.startswith("|"):
        cells=line.strip("|").split("|")
        if all(re.fullmatch(r"\s*:?-+:?\s*",cell) for cell in cells):
            continue
        if not table:
            content.append("<table>")
            table=True
        content.append("<tr>"+"".join("<td>"+inline(cell.strip())+"</td>" for cell in cells)+"</tr>")
        continue
    if table:
        content.append("</table>")
        table=False
    if not line.strip():
        continue
    if line.startswith("#"):
        depth=min(len(line)-len(line.lstrip("#")),4)
        content.append(f"<h{depth}>"+inline(line.lstrip("# "))+f"</h{depth}>")
    else:
        content.append("<p>"+inline(line.removeprefix("- "))+"</p>")
if table:
    content.append("</table>")
checks="".join("<li>"+html.escape(item)+"</li>" for item in desktop["checks"])
report="""<!doctype html><html lang="zh-CN"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>MUSP Studio 0.1.0 验收报告</title><style>
body{margin:0;background:#eef3f8;color:#203248;font:15px/1.8 'Microsoft YaHei UI',sans-serif}main{max-width:1080px;margin:32px auto;padding:38px;background:white;border:1px solid #d6e0eb;border-radius:12px}h1{font-size:28px}h2{border-top:1px solid #d6e0eb;padding-top:22px;margin-top:32px}code{font:13px Consolas,monospace;background:#eef3f8;padding:3px 5px;overflow-wrap:anywhere}table{border-collapse:collapse;width:100%;font-size:13px}td{border:1px solid #d6e0eb;padding:10px;vertical-align:top}tr:first-child{background:#eaf1f8;font-weight:bold}.verified{padding:20px;background:#e7f7ef;border-left:4px solid #11834b}a{color:#075fc4}details{padding:16px;background:#f5f8fb}img{width:100%;border:1px solid #d6e0eb;margin-top:12px}pre{white-space:pre-wrap;overflow-wrap:anywhere;font:12px/1.6 Consolas,monospace}</style><main>
<div class="verified"><strong>Windows NSIS 安装、真实UDP自测、桌面功能和卸载验证通过</strong><br>安装包包含WebView2离线运行时；全新虚拟机与现场硬件联调未验证。</div>
"""+"".join(content)+"<h2>安装版界面检查</h2><ul>"+checks+"</ul><p>实际页面来源：<code>"+html.escape(desktop["url"])+"</code>；未处理脚本错误："+str(len(desktop["pageErrors"]))+"</p>"
report+="<details><summary>原始安装验证结果</summary><pre>"+html.escape(json.dumps(install,ensure_ascii=False,indent=2))+"</pre></details>"
for n,name in enumerate(["工作台","协议设计","设备管理","交互流程","报文监控","通话资源","模板库"],1):
    report+=f'<details><summary>{n}. {name}（真实安装版）</summary><img src="screenshots/page-{n}.png" alt="{name}"></details>'
report+="<p><a href=\"MUSP-Studio_0.1.0_x64-setup.exe\">Windows安装包</a> · <a href=\"SHA256SUMS.txt\">SHA256清单</a> · <a href=\"README_安装使用.md\">安装使用说明</a></p></main></html>"
(release/"test-report.html").write_text(report,encoding="utf-8")
print(release/"test-report.html")
