import { useEffect, useState } from "react";
import { ChevronDown, Pencil, Plus, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Pagination } from "@/components/ui/pagination";
import { Badge } from "@/components/ui/badge";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { cn } from "@/lib/utils";
import type { DictItem, DictSetView } from "@/types";

export function DictionarySection({ dictionary, open, selected, onToggle, onEditSet, onDeleteSet, onEditItem, onDeleteItem }: {
  dictionary: DictSetView;
  open: boolean;
  selected: boolean;
  onToggle: () => void;
  onEditSet: () => void;
  onDeleteSet: () => void;
  onEditItem: (item: DictItem | null) => void;
  onDeleteItem: (item: DictItem) => void;
}) {
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const totalPages = Math.max(1, Math.ceil(dictionary.items.length / pageSize));
  const safePage = Math.min(page, totalPages);
  useEffect(() => { setPage(p => Math.min(p, totalPages)); }, [totalPages]);
  return <section id={`dict-panel-${dictionary.id}`} className={cn("scroll-mt-2 rounded-md border bg-card", selected && "border-primary/60")}>
    <div className="dictionary-card-header flex items-center gap-2 px-3 py-1 text-sm">
      <button type="button" aria-expanded={open} aria-controls={`dict-content-${dictionary.id}`} aria-label={`${open ? "折叠" : "展开"}字典：${dictionary.name}`} onClick={onToggle} className="flex min-w-0 flex-1 items-center gap-2 text-left">
        <ChevronDown className={cn("h-4 w-4 shrink-0 transition-transform", !open && "-rotate-90")} />
        <span className="truncate font-medium" title={[dictionary.name, dictionary.group_ref == null ? "全局共享" : `${dictionary.system_name ?? ""} / ${dictionary.group_name ?? ""}`, dictionary.key, dictionary.description].filter(Boolean).join(" · ")}>{dictionary.name}</span>
        <span className="shrink-0 text-xs text-muted-foreground">{dictionary.byte_len}B · {dictionary.items.length} 项</span>
      </button>
      <Button variant="outline" size="sm" className="h-7 px-2 text-xs" onClick={() => onEditItem(null)}><Plus className="h-3.5 w-3.5" />添加条目</Button>
      <Button variant="ghost" size="icon" className="h-7 w-7" title="编辑字典" onClick={onEditSet}><Pencil className="h-3.5 w-3.5" /></Button>
      <Button variant="ghost" size="icon" title="删除字典" className="h-7 w-7 text-destructive" onClick={onDeleteSet}><Trash2 className="h-3.5 w-3.5" /></Button>
    </div>
    {open && <div id={`dict-content-${dictionary.id}`} className="overflow-hidden rounded-b-md border-t">
        <Table aria-label={`${dictionary.name}条目`} className="min-w-[480px]" containerClassName="max-h-96">
          <TableHeader><TableRow><TableHead className="h-8">值（十六进制）</TableHead><TableHead className="h-8">含义</TableHead><TableHead className="h-8">排序</TableHead><TableHead className="h-8 text-right">操作</TableHead></TableRow></TableHeader>
          <TableBody>
            {dictionary.items.length === 0 && <TableRow><TableCell colSpan={4} className="h-20 text-center text-muted-foreground">暂无条目</TableCell></TableRow>}
            {dictionary.items.slice((safePage - 1) * pageSize, safePage * pageSize).map(item => <TableRow key={item.id}>
              <TableCell className="px-3 py-1 font-mono">{item.value}{item.id === dictionary.items[0]?.id && <Badge variant="secondary" className="ml-2">默认</Badge>}</TableCell>
              <TableCell className="px-3 py-1">{item.label}</TableCell><TableCell className="px-3 py-1">{item.sort_order}</TableCell>
              <TableCell className="px-3 py-1"><div className="flex justify-end"><Button variant="ghost" size="icon" className="h-6 w-6" title="编辑条目" onClick={() => onEditItem(item)}><Pencil className="h-3.5 w-3.5" /></Button><Button variant="ghost" size="icon" title="删除条目" className="h-6 w-6 text-destructive" onClick={() => onDeleteItem(item)}><Trash2 className="h-3.5 w-3.5" /></Button></div></TableCell>
            </TableRow>)}
          </TableBody>
        </Table>
      <Pagination label={`${dictionary.name}条目`} total={dictionary.items.length} page={safePage} pageSize={pageSize} size={pageSize}
        onPageChange={setPage} onSizeChange={size => { if (size !== 'auto') setPageSize(size); setPage(1); }} />
    </div>}
  </section>;
}
