/**
 * @file DictionaryManager.tsx
 * @description 字典管理页: 左侧系统/分组树, 右侧查询与字典条目维护
 * @module 字典管理
 */
import {
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { toast } from "sonner";
import {
  BookMarked,
  ChevronDown,
  Folder,
  Globe,
  Plus,
  Search,
  Server,
} from "lucide-react";
import { DictionarySection } from "@/components/DictionarySection";
import { WorkspaceActivityContext } from "@/components/layout/WorkspaceActivity";
import { Button } from "@/components/ui/button";
import { Pagination, type PageSize } from "@/components/ui/pagination";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  deleteDictItem,
  deleteDictSet,
  listDictSets,
  listSystems,
  upsertDictItem,
  upsertDictSet,
} from "@/lib/api";
import { cn } from "@/lib/utils";
import type {
  DictItem,
  DictSetView,
  ProtocolGroup,
  ProtocolSystem,
} from "@/types";
import { listGroups } from "@/lib/api";

/** 字典表单状态（byte_len 用字符串以便输入框受控） */
interface SetFormState {
  /** ASCII 标识, 模板中 {dict:key} 引用它 */
  key: string;
  /** 中文显示名 */
  name: string;
  /** "global" 表示全局共享, 否则为分组 id 字符串（分组专属） */
  scope: string;
  /** 归属导航用: 所选系统 id 字符串（"none" = 未选, 不直接入库） */
  scopeSys: string;
  /** 每个条目的字节长度 */
  byte_len: string;
  description: string;
}

/** 条目表单状态 */
interface ItemFormState {
  /** 十六进制值, 可含空格 */
  value: string;
  /** 含义 */
  label: string;
}

const emptySetForm: SetFormState = {
  key: "",
  name: "",
  scope: "global",
  scopeSys: "none",
  byte_len: "1",
  description: "",
};

const emptyItemForm: ItemFormState = { value: "", label: "" };

/** 字典树节点: 系统一级（全局共享为伪系统, 字典直接挂）, 分组二级 */
interface SetSysNode {
  /** 折叠标识 key */
  key: string;
  name: string;
  global?: boolean;
  /** 全局节点: 字典直接挂系统级 */
  sets?: DictSetView[];
  /** 非全局节点: 分组行 */
  groupRows?: { key: string; name: string; sets: DictSetView[] }[];
}

/**
 * 把字典列表构建为 系统 → 分组 两级树（全局共享字典为顶层伪系统）
 *
 * @param sets - 字典列表（后端已排好序）
 */
function buildSetTree(sets: DictSetView[]): SetSysNode[] {
  const out: SetSysNode[] = [];
  const globals = sets.filter((s) => s.group_ref == null);
  if (globals.length) {
    out.push({
      key: "sys:__global__",
      name: "全局共享",
      global: true,
      sets: globals,
    });
  }
  const bySystem = new Map<string, DictSetView[]>();
  for (const s of sets.filter((x) => x.group_ref != null)) {
    const sys = s.system_name ?? "系统信息缺失";
    if (!bySystem.has(sys)) bySystem.set(sys, []);
    bySystem.get(sys)!.push(s);
  }
  for (const [sysName, list] of bySystem) {
    const byGroup = new Map<string, DictSetView[]>();
    for (const s of list) {
      const gk = String(s.group_ref);
      if (!byGroup.has(gk)) byGroup.set(gk, []);
      byGroup.get(gk)!.push(s);
    }
    out.push({
      key: `sys:${sysName}`,
      name: sysName,
      groupRows: [...byGroup.entries()].map(([gk, gsets]) => ({
        key: `grp:${gk}`,
        name: gsets[0]?.group_name ?? "未分组",
        sets: gsets,
      })),
    });
  }
  return out;
}

/**
 * 字典管理页
 *
 * 字典是两层结构: 字典（如「设备类型」）→ 条目（01=温度传感器 / 02=湿度传感器）。
 * 字典可挂在某个系统下实现隔离, 也可留作全局共享。协议字段在「协议管理」里
 * 绑定字典后, 取值即变成条目下拉。
 */
export function DictionaryManager() {
  const { active } = useContext(WorkspaceActivityContext);
  /** 全部字典（含条目） */
  const [sets, setSets] = useState<DictSetView[]>([]);
  /** 协议系统列表, 供字典选择归属 */
  const [systems, setSystems] = useState<ProtocolSystem[]>([]);
  /** 全部协议分组, 供字典选择归属(两级联动第二级); 与下方过滤 memo 的 groups 区分 */
  const [allGroups, setAllGroups] = useState<ProtocolGroup[]>([]);
  /** 当前选中的字典 id */
  const [selectedId, setSelectedId] = useState<number | null>(null);
  /** 右侧查询关键词, 匹配字典名/Key/条目含义/条目值 */
  const [search, setSearch] = useState("");
  const [scope, setScope] = useState("all");
  const [page, setPage] = useState(1);
  const [autoSize, setAutoSize] = useState(1);
  const [pageSizeRaw, setPageSizeRaw] = useState<PageSize>("auto");
  const dictionariesPerPage = pageSizeRaw === "auto" ? autoSize : pageSizeRaw;
  const pageSizeRef = useRef(1);
  const cardListRef = useRef<HTMLDivElement>(null);
  /** 选中字典被哪些协议引用（协议ID列表） */
  const [openSets, setOpenSets] = useState<Set<number>>(new Set());
  const [itemOwnerId, setItemOwnerId] = useState<number | null>(null);

  /** 字典新增/编辑弹窗是否打开 */
  const [setDialogOpen, setSetDialogOpen] = useState(false);
  /** 正在编辑的字典, null 表示新增模式 */
  const [setEditing, setSetEditing] = useState<DictSetView | null>(null);
  const [setForm, setSetForm] = useState<SetFormState>(emptySetForm);

  /** 条目表分页(字典切换时重置到第一页) */
  /** 条目新增/编辑弹窗是否打开 */
  const [itemDialogOpen, setItemDialogOpen] = useState(false);
  /** 正在编辑的条目, null 表示新增模式 */
  const [itemEditing, setItemEditing] = useState<DictItem | null>(null);
  const [itemForm, setItemForm] = useState<ItemFormState>(emptyItemForm);

  /** 保存中（禁用按钮防重复提交） */
  const [saving, setSaving] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const [s, sys, g] = await Promise.all([
        listDictSets(),
        listSystems(),
        listGroups(),
      ]);
      setSets(s);
      setSystems(sys);
      setAllGroups(g);
      // 选中项被删除时回落到第一张字典
      setSelectedId((cur) =>
        cur != null && s.some((x) => x.id === cur) ? cur : (s[0]?.id ?? null),
      );
      return s;
    } catch (e) {
      toast.error("加载字典失败", { description: String(e) });
    }
  }, []);

  useEffect(() => {
    if (active) void refresh();
  }, [active, refresh]);

  const itemOwner = sets.find((s) => s.id === itemOwnerId) ?? null;

  const selectScope = (key: string) => {
    setScope(key);
    setPage(1);
  };

  /** 搜索过滤后的字典集合 */
  const filteredSets = useMemo(() => {
    const kw = search.trim().toLowerCase();
    if (!kw) return sets;
    return sets.filter(
      (s) =>
        s.name.toLowerCase().includes(kw) ||
        s.key.toLowerCase().includes(kw) ||
        s.items.some(
          (i) =>
            i.label.toLowerCase().includes(kw) ||
            i.value.toLowerCase().includes(kw),
        ),
    );
  }, [sets, search]);

  /** 系统 → 分组 两级树 */
  const tree = useMemo(() => buildSetTree(sets), [sets]);
  const scopedSets = useMemo(
    () =>
      filteredSets.filter((s) => {
        if (scope === "all") return true;
        if (scope === "sys:__global__") return s.group_ref == null;
        if (scope.startsWith("grp:"))
          return String(s.group_ref) === scope.slice(4);
        return (
          s.group_ref != null &&
          `sys:${s.system_name ?? "系统信息缺失"}` === scope
        );
      }),
    [filteredSets, scope],
  );
  const totalPages = Math.max(
    1,
    Math.ceil(scopedSets.length / dictionariesPerPage),
  );
  const safePage = Math.min(page, totalPages);
  const pageSets = scopedSets.slice(
    (safePage - 1) * dictionariesPerPage,
    safePage * dictionariesPerPage,
  );
  useEffect(() => {
    setPage((p) => Math.min(p, totalPages));
  }, [totalPages]);

  // Measure the actual collapsed header, even when a card's entries are expanded.
  // The list keeps normal scrolling for expanded content; pagination fits collapsed cards.
  useLayoutEffect(() => {
    const list = cardListRef.current;
    if (!list) return;
    const measure = () => {
      if (list.clientHeight === 0) return; // An inactive workspace tab has no layout.
      const headers = [
        ...list.querySelectorAll<HTMLElement>(".dictionary-card-header"),
      ];
      if (!headers.length) return;
      const cardHeight = Math.max(
        ...headers.map((header) => header.getBoundingClientRect().height + 2),
      );
      if (cardHeight <= 2) return;
      const gap = parseFloat(getComputedStyle(list).rowGap) || 0;
      const next = Math.max(
        1,
        Math.floor((list.clientHeight + gap - 1) / (cardHeight + gap)),
      );
      const previous = pageSizeRef.current;
      if (next === previous) return;
      pageSizeRef.current = next;
      setAutoSize(next);
      if (pageSizeRaw === "auto")
        setPage((current) => Math.floor(((current - 1) * previous) / next) + 1);
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(list);
    list
      .querySelectorAll(".dictionary-card-header")
      .forEach((header) => observer.observe(header));
    return () => observer.disconnect();
  }, [scopedSets, safePage, pageSizeRaw]);

  /** 展开的节点 key（白名单: 默认全部收起, 点开/选中链路才展开; 搜索时忽略全展开） */
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const toggleNode = (key: string) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });

  // 选中字典变化时自动展开其所在链路（系统+分组）, 避免选中项藏在收起节点里
  useEffect(() => {
    if (selectedId == null) return;
    const s = sets.find((x) => x.id === selectedId);
    if (!s) return;
    setExpanded((prev) => {
      const next = new Set(prev);
      if (s.group_ref == null) {
        next.add("sys:__global__");
      } else {
        next.add(`sys:${s.system_name ?? "系统信息缺失"}`);
        next.add(`grp:${s.group_ref}`);
      }
      return next;
    });
  }, [selectedId, sets]);

  /**
   * 打开字典弹窗
   *
   * @param target - 要编辑的字典; 传 null 进入新增模式
   */
  const openSetDialog = (target: DictSetView | null) => {
    setSetEditing(target);
    setSetForm(
      target
        ? {
            key: target.key,
            name: target.name,
            scope:
              target.group_ref == null ? "global" : String(target.group_ref),
            scopeSys:
              target.group_ref != null
                ? String(
                    allGroups.find((g) => g.id === target.group_ref)
                      ?.system_ref ?? "none",
                  )
                : "none",
            byte_len: String(target.byte_len),
            description: target.description ?? "",
          }
        : scope.startsWith("grp:")
          ? {
              ...emptySetForm,
              scope: scope.slice(4),
              scopeSys: String(
                allGroups.find((g) => g.id === Number(scope.slice(4)))
                  ?.system_ref ?? "none",
              ),
            }
          : emptySetForm,
    );
    setSetDialogOpen(true);
  };

  const handleSaveSet = async () => {
    if (!setForm.name.trim()) {
      toast.error("请填写字典名称");
      return;
    }
    const len = Number(setForm.byte_len);
    if (!Number.isInteger(len) || len < 1 || len > 64) {
      toast.error("字节长度需为 1-64 的整数");
      return;
    }
    setSaving(true);
    try {
      if (setForm.scopeSys !== "none" && setForm.scope === "global") {
        toast.error(
          "已选择系统但未选分组: 请在「所属分组」中选择, 或将归属改回「全局共享」",
        );
        return;
      }
      const saved = await upsertDictSet(setEditing?.id ?? null, {
        // 新建留空 → 后端自动生成内部标识; 编辑时原样回传(后端规整后不变)
        key: setEditing ? setForm.key : "",
        name: setForm.name.trim(),
        system_ref: null,
        group_ref: setForm.scope === "global" ? null : Number(setForm.scope),
        byte_len: len,
        description: setForm.description.trim() || null,
      });
      toast.success(`字典「${saved.name}」已保存`);
      setSetDialogOpen(false);
      const latest = await refresh();
      setSelectedId(saved.id);
      setSearch("");
      setScope(
        saved.group_ref == null ? "sys:__global__" : `grp:${saved.group_ref}`,
      );
      const siblings =
        latest?.filter((s) => s.group_ref === saved.group_ref) ?? [];
      setPage(
        Math.floor(
          Math.max(
            0,
            siblings.findIndex((s) => s.id === saved.id),
          ) / dictionariesPerPage,
        ) + 1,
      );
    } catch (e) {
      toast.error("保存失败", { description: String(e) });
    } finally {
      setSaving(false);
    }
  };

  const handleDeleteSet = async (s: DictSetView) => {
    if (
      !window.confirm(`确定删除字典「${s.name}」及其 ${s.items.length} 个条目?`)
    )
      return;
    try {
      await deleteDictSet(s.id);
      toast.success("字典已删除");
      await refresh();
    } catch (e) {
      // 后端在仍被协议引用时会拒绝, 直接把原因透出来
      toast.error("删除失败", { description: String(e) });
    }
  };

  /**
   * 打开条目弹窗
   *
   * @param target - 要编辑的条目; 传 null 进入新增模式
   */
  const openItemDialog = (target: DictItem | null, owner: DictSetView) => {
    setItemOwnerId(owner.id);
    setItemEditing(target);
    setItemForm(
      target ? { value: target.value, label: target.label } : emptyItemForm,
    );
    setItemDialogOpen(true);
  };

  const handleSaveItem = async () => {
    if (!itemOwner) return;
    if (!itemForm.value.trim()) {
      toast.error("请填写条目值");
      return;
    }
    if (!itemForm.label.trim()) {
      toast.error("请填写条目含义");
      return;
    }
    setSaving(true);
    try {
      await upsertDictItem(itemEditing?.id ?? null, {
        set_ref: itemOwner.id,
        value: itemForm.value.trim(),
        label: itemForm.label.trim(),
        // 新增条目排到末尾; 编辑时保持原排序
        sort_order: itemEditing?.sort_order ?? itemOwner.items.length,
      });
      toast.success("条目已保存");
      setItemDialogOpen(false);
      await refresh();
    } catch (e) {
      toast.error("保存失败", { description: String(e) });
    } finally {
      setSaving(false);
    }
  };

  const handleDeleteItem = async (item: DictItem) => {
    if (!window.confirm(`确定删除条目「${item.value} = ${item.label}」?`))
      return;
    try {
      await deleteDictItem(item.id);
      toast.success("条目已删除");
      await refresh();
    } catch (e) {
      toast.error("删除失败", { description: String(e) });
    }
  };

  /** 条目值输入框的示例文本, 按字典字节长度生成 */
  const valuePlaceholder = itemOwner
    ? Array.from({ length: itemOwner.byte_len }, (_, i) =>
        i === itemOwner.byte_len - 1 ? "01" : "00",
      ).join(" ")
    : "01";

  return (
    <div className="dictionary-manager flex h-full min-h-0 p-6">
      {/* 左侧: 系统/分组树 */}
      <div
        role="navigation"
        aria-label="字典树"
        className="mr-4 flex min-h-0 w-56 flex-none flex-col rounded-xl border bg-card"
      >
        <div className="flex-1 overflow-y-auto p-2 text-sm">
          <div className="mb-1 flex items-center gap-1">
            <button
              type="button"
              aria-current={scope === "all" ? "page" : undefined}
              className={cn(
                "flex min-w-0 flex-1 items-center gap-2 rounded-md px-2 py-1.5 text-left text-xs",
                scope === "all" ? "bg-secondary" : "hover:bg-accent",
              )}
              onClick={() => selectScope("all")}
            >
              <BookMarked className="h-3.5 w-3.5" />
              全部字典
              <span className="ml-auto text-muted-foreground">
                {sets.length}
              </span>
            </button>
            <Button
              variant="ghost"
              size="icon"
              className="h-7 w-7 shrink-0"
              title="新增字典"
              aria-label="新增字典"
              onClick={() => openSetDialog(null)}
            >
              <Plus />
            </Button>
          </div>
          {tree.map((sys) => {
            const sysOpen = expanded.has(sys.key);
            const sysCount = sys.global
              ? sys.sets!.length
              : sys.groupRows!.reduce((a, g) => a + g.sets.length, 0);
            return (
              <div key={sys.key} className="mb-1">
                <div
                  className={cn(
                    "flex items-center rounded-md",
                    scope === sys.key ? "bg-secondary" : "hover:bg-accent",
                  )}
                >
                  {!sys.global && (
                    <button
                      type="button"
                      aria-label={`${sysOpen ? "收起" : "展开"}系统：${sys.name}`}
                      aria-expanded={sysOpen}
                      className="p-1.5"
                      onClick={() => toggleNode(sys.key)}
                    >
                      <ChevronDown
                        className={cn(
                          "h-3.5 w-3.5 text-muted-foreground transition-transform",
                          !sysOpen && "-rotate-90",
                        )}
                      />
                    </button>
                  )}
                  <button
                    type="button"
                    aria-current={scope === sys.key ? "page" : undefined}
                    className={cn(
                      "flex min-w-0 flex-1 items-center gap-1.5 py-1.5 pr-2 text-left",
                      sys.global && "pl-2",
                    )}
                    onClick={() => {
                      selectScope(sys.key);
                      setExpanded((prev) => new Set(prev).add(sys.key));
                    }}
                  >
                    {sys.global ? (
                      <Globe className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                    ) : (
                      <Server className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                    )}
                    <span className="truncate text-xs font-semibold">
                      {sys.name}
                    </span>
                    <span className="ml-auto shrink-0 text-[11px] text-muted-foreground">
                      {sysCount}
                    </span>
                  </button>
                </div>
                {sysOpen &&
                  !sys.global &&
                  sys.groupRows!.map((g) => (
                    <button
                      key={g.key}
                      type="button"
                      aria-current={scope === g.key ? "page" : undefined}
                      className={cn(
                        "flex w-full items-center gap-1.5 rounded-md py-1.5 pl-7 pr-2 text-left",
                        scope === g.key ? "bg-secondary" : "hover:bg-accent",
                      )}
                      onClick={() => selectScope(g.key)}
                    >
                      <Folder className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                      <span className="truncate text-xs">{g.name}</span>
                      <span className="ml-auto shrink-0 text-[11px] text-muted-foreground">
                        {g.sets.length}
                      </span>
                    </button>
                  ))}
              </div>
            );
          })}
        </div>
      </div>

      <div className="flex min-h-0 min-w-0 flex-1 flex-col">
        <div className="mb-3 flex shrink-0 flex-wrap items-center justify-between gap-3">
          <div className="relative min-w-48 max-w-md flex-1">
            <Search className="pointer-events-none absolute left-2.5 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
            <Input
              aria-label="查询字典"
              className="h-8 pl-8 text-sm"
              placeholder="查询字典名 / Key / 条目"
              value={search}
              onChange={(e) => {
                setSearch(e.target.value);
                setPage(1);
              }}
            />
          </div>
          <div className="ml-auto flex shrink-0 gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={() =>
                setOpenSets(
                  (prev) => new Set([...prev, ...pageSets.map((s) => s.id)]),
                )
              }
            >
              展开本页
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={() =>
                setOpenSets(
                  (prev) =>
                    new Set(
                      [...prev].filter(
                        (id) => !pageSets.some((s) => s.id === id),
                      ),
                    ),
                )
              }
            >
              收起本页
            </Button>
          </div>
        </div>
        <div
          ref={cardListRef}
          role="region"
          aria-label="字典列表"
          className="dictionary-cards min-h-0 flex-1 overflow-y-auto pr-1"
        >
          {pageSets.map((s) => (
            <DictionarySection
              key={s.id}
              dictionary={s}
              open={openSets.has(s.id)}
              selected={selectedId === s.id}
              onToggle={() => {
                if (!openSets.has(s.id)) setSelectedId(s.id);
                setOpenSets((prev) => {
                  const next = new Set(prev);
                  if (next.has(s.id)) next.delete(s.id);
                  else next.add(s.id);
                  return next;
                });
              }}
              onEditSet={() => openSetDialog(s)}
              onDeleteSet={() => void handleDeleteSet(s)}
              onEditItem={(item) => openItemDialog(item, s)}
              onDeleteItem={(item) => void handleDeleteItem(item)}
            />
          ))}
          {scopedSets.length === 0 && (
            <p className="rounded border border-dashed p-8 text-center text-sm text-muted-foreground">
              {sets.length ? "没有匹配的字典" : "暂无字典"}
            </p>
          )}
        </div>
        <Pagination
          className="mt-2 shrink-0"
          label="字典"
          buttonSuffix="字典"
          total={scopedSets.length}
          page={safePage}
          pageSize={dictionariesPerPage}
          size={pageSizeRaw}
          autoSize={autoSize}
          onPageChange={setPage}
          onSizeChange={(size) => {
            setPageSizeRaw(size);
            setPage(1);
          }}
        />
      </div>

      {/* 字典编辑弹窗 */}
      <Dialog open={setDialogOpen} onOpenChange={setSetDialogOpen}>
        <DialogContent className="max-w-md" aria-describedby={undefined}>
          <DialogHeader>
            <DialogTitle>{setEditing ? "编辑字典" : "新增字典"}</DialogTitle>
          </DialogHeader>
          <div className="grid gap-4">
            <div className="grid gap-2">
              <Label htmlFor="ds-name">名称 *</Label>
              <Input
                id="ds-name"
                value={setForm.name}
                placeholder="如 设备类型"
                onChange={(e) =>
                  setSetForm({ ...setForm, name: e.target.value })
                }
              />
            </div>
            <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
              <div className="grid gap-2">
                <Label htmlFor="ds-scope">归属</Label>
                <Select
                  value={
                    setForm.scope === "global"
                      ? setForm.scopeSys === "none"
                        ? "global"
                        : `sys:${setForm.scopeSys}`
                      : setForm.scope
                  }
                  onValueChange={(v) => {
                    if (v === "global") {
                      setSetForm((f) => ({
                        ...f,
                        scope: "global",
                        scopeSys: "none",
                      }));
                    } else if (v.startsWith("sys:")) {
                      // 选系统档: 进入分组选择, 字典档位在选中具体分组前保持全局
                      setSetForm((f) => ({
                        ...f,
                        scopeSys: v.slice(4),
                        scope: "global",
                      }));
                    }
                  }}
                >
                  <SelectTrigger id="ds-scope">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="global">全局共享</SelectItem>
                    {systems.map((s) => (
                      <SelectItem key={s.id} value={`sys:${s.id}`}>
                        {s.name}
                      </SelectItem>
                    ))}
                    {setForm.scope !== "global" &&
                      (() => {
                        const g = allGroups.find(
                          (x) => String(x.id) === setForm.scope,
                        );
                        if (!g) return null;
                        const sys = systems.find(
                          (ss) => ss.id === g.system_ref,
                        );
                        return (
                          <SelectItem value={setForm.scope}>
                            {sys ? `${sys.name} / ${g.name}` : g.name}
                          </SelectItem>
                        );
                      })()}
                  </SelectContent>
                </Select>
              </div>
              {setForm.scopeSys !== "none" && (
                <div className="grid gap-2">
                  <Label htmlFor="ds-group">所属分组</Label>
                  <Select
                    value={
                      setForm.scope === "global" ? undefined : setForm.scope
                    }
                    onValueChange={(v) =>
                      setSetForm((f) => ({ ...f, scope: v }))
                    }
                  >
                    <SelectTrigger id="ds-group">
                      <SelectValue placeholder="选择分组" />
                    </SelectTrigger>
                    <SelectContent>
                      {allGroups
                        .filter(
                          (g) => String(g.system_ref) === setForm.scopeSys,
                        )
                        .map((g) => (
                          <SelectItem key={g.id} value={String(g.id)}>
                            {g.name}
                          </SelectItem>
                        ))}
                    </SelectContent>
                  </Select>
                </div>
              )}
              <div className="grid gap-2">
                <Label htmlFor="ds-len">字节长度 *</Label>
                <Input
                  id="ds-len"
                  type="number"
                  min={1}
                  max={64}
                  value={setForm.byte_len}
                  onChange={(e) =>
                    setSetForm({ ...setForm, byte_len: e.target.value })
                  }
                />
              </div>
            </div>
            <div className="grid gap-2">
              <Label htmlFor="ds-desc">说明</Label>
              <Input
                id="ds-desc"
                value={setForm.description}
                placeholder="可选"
                onChange={(e) =>
                  setSetForm({ ...setForm, description: e.target.value })
                }
              />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setSetDialogOpen(false)}>
              取消
            </Button>
            <Button onClick={handleSaveSet} disabled={saving}>
              {saving ? "保存中..." : "保存"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 条目编辑弹窗 */}
      <Dialog open={itemDialogOpen} onOpenChange={setItemDialogOpen}>
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>{itemEditing ? "编辑条目" : "添加条目"}</DialogTitle>
            <DialogDescription>
              所属字典「{itemOwner?.name}」· 每个条目 {itemOwner?.byte_len} 字节
            </DialogDescription>
          </DialogHeader>
          <div className="grid gap-4">
            <div className="grid gap-2">
              <Label htmlFor="di-value">值（十六进制）*</Label>
              <Input
                id="di-value"
                className="font-mono"
                value={itemForm.value}
                placeholder={`如 ${valuePlaceholder}`}
                onChange={(e) =>
                  setItemForm({ ...itemForm, value: e.target.value })
                }
              />
            </div>
            <div className="grid gap-2">
              <Label htmlFor="di-label">含义 *</Label>
              <Input
                id="di-label"
                value={itemForm.label}
                placeholder="如 温度传感器"
                onChange={(e) =>
                  setItemForm({ ...itemForm, label: e.target.value })
                }
              />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setItemDialogOpen(false)}>
              取消
            </Button>
            <Button onClick={handleSaveItem} disabled={saving}>
              {saving ? "保存中..." : "保存"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
