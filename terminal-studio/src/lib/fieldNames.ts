/**
 * 字段名工具: 协议内允许中文字段名重复, 整帧字段表从第二个同名者起加 `#2/#3` 唯一键,
 * 引擎的参数/回填/提取均按该键索引; 展示时用 displayFieldName 转回可读形式。
 */

/** 重名字段加 `#k` 后缀（与后端 dedupe_field_names 同规则; 返回新数组） */
export function dedupeNames(names: string[]): string[] {
  const reserved = new Set(names);
  const used = new Set<string>();
  return names.map((nm) => {
    let unique = nm;
    if (used.has(unique)) {
      let suffix = 2;
      while (reserved.has(`${nm}#${suffix}`) || used.has(`${nm}#${suffix}`)) suffix++;
      unique = `${nm}#${suffix}`;
    }
    used.add(unique);
    return unique;
  });
}

/** 展示用字段名: `序号#2` → `序号(2)`; 无后缀原样返回 */
export function displayFieldName(name: string): string {
  const m = name.match(/^(.*)#(\d+)$/);
  return m ? `${m[1]}(${m[2]})` : name;
}
