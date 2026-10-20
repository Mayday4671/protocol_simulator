// Browser regression checks. Run against the in-memory UI preview through CUA;
// these checks never call the application backend or send network messages.
export async function expectExpandedBranch(page, field, value) {
  const branch = page.playwright.locator(
    `[data-branch-group="${field}"][data-branch-value="${value}"]`,
  );
  const expanded = await branch
    .getByRole("button")
    .first()
    .getAttribute("aria-expanded");
  if (expanded !== "true")
    throw new Error(`追加字段后 ${field}=${value} 仍折叠`);
}

export async function expectCompactBranchEditor(page) {
  const bounds = await page.playwright
    .getByRole("combobox", { name: "分支条件字段", exact: true })
    .evaluate((element) => element.getBoundingClientRect().toJSON());
  if (bounds.width > 320)
    throw new Error(`分支条件选择器独占整行：${bounds.width}px`);
}

export async function expectSuggestionsDismissed(page) {
  const openMenus = await page.playwright.getByRole("listbox").count();
  if (openMenus !== 0) throw new Error("按 Escape 后建议菜单未关闭");
}

export async function expectRuntimeColumns(page, minimum = 2) {
  const columns = await page.playwright
    .locator(".runtime-fields")
    .first()
    .evaluate(
      (element) =>
        getComputedStyle(element).gridTemplateColumns.split(" ").length,
    );
  if (columns < minimum)
    throw new Error(`普通字段仅 ${columns} 列，预期至少 ${minimum} 列`);
}

export async function expectRecordColumns(page, expected) {
  const columns = await page.playwright
    .getByRole("group", { name: "第 1 条", exact: true })
    .evaluate(
      (element) =>
        getComputedStyle(element).gridTemplateColumns.split(" ").length,
    );
  if (columns !== expected)
    throw new Error(`重复记录应按 ${expected} 列排列，实际 ${columns} 列`);
}

export async function expectCompactDeviceFilter(page) {
  const width = await page.playwright
    .getByRole("combobox", { name: "筛选设备系统", exact: true })
    .evaluate((element) => element.getBoundingClientRect().width);
  if (width > 360) throw new Error(`设备筛选框占满整行：${width}px`);
}
