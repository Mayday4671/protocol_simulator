export interface XmlCheck {
  ok: boolean;
  msg: string;
  line?: number;
  column?: number;
}

/** Syntax only. Import semantics remain the responsibility of the Rust importer. */
export function checkXml(src: string): XmlCheck {
  if (!src.trim()) return { ok: false, msg: "内容为空", line: 1, column: 1 };
  try {
    const doc = new DOMParser().parseFromString(src, "application/xml");
    const error = doc.getElementsByTagName("parsererror")[0];
    if (error) {
      const detail = (error.textContent ?? "XML 语法错误").replace(/\s+/g, " ").trim();
      const location = detail.match(/(?:at|on) line (\d+)(?: at)? column (\d+)/i);
      const line = location ? Number(location[1]) : undefined;
      const column = location ? Number(location[2]) : undefined;
      const reason = detail.replace(/^.*?error (?:at|on) line \d+(?: at)? column \d+:?\s*/i, "")
        .replace(/Below is a rendering.*$/i, "").trim();
      return { ok: false, msg: `${line ? `第 ${line} 行，第 ${column} 列：` : ""}${reason.slice(0, 200)}`, line, column };
    }
    return { ok: true, msg: `语法正确 · ${doc.getElementsByTagName("protocol").length} 条协议 / ${doc.getElementsByTagName("dict").length} 张字典` };
  } catch {
    return { ok: false, msg: "XML 解析失败" };
  }
}
