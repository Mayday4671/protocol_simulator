import { Button } from '@/components/ui/button';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { cn } from '@/lib/utils';

export type PageSize = number | 'auto';

/** Shared with protocol management: count, numbered pages and page-size selection. */
export function Pagination({ total, page, pageSize, size, autoSize, onPageChange, onSizeChange, label, buttonSuffix = '', className }: {
  total: number;
  page: number;
  pageSize: number;
  size: PageSize;
  autoSize?: number;
  onPageChange: (page: number) => void;
  onSizeChange: (size: PageSize) => void;
  label: string;
  buttonSuffix?: string;
  className?: string;
}) {
  const pages = Math.max(1, Math.ceil(total / Math.max(1, pageSize)));
  const current = Math.max(1, Math.min(page, pages));
  const numbers = [...new Set([1, current - 1, current, current + 1, pages])].filter(n => n >= 1 && n <= pages).sort((a, b) => a - b);
  return <nav aria-label={`${label}分页`} className={cn('flex flex-wrap items-center justify-between gap-2 border-t px-3 py-2 text-xs text-muted-foreground', className)}>
    <span>共 {total} 条 · 第 {current}/{pages} 页</span>
    <div className="flex flex-wrap items-center gap-1">
      <Button variant="outline" size="sm" className="h-7" aria-label={`上一页${buttonSuffix}`} disabled={current <= 1} onClick={() => onPageChange(current - 1)}>上一页</Button>
      {numbers.map((n, index) => <span key={n} className="flex items-center">
        {index > 0 && n - numbers[index - 1] > 1 && <span className="px-1">…</span>}
        <Button variant={n === current ? 'default' : 'outline'} size="sm" className="h-7 w-7 p-0" aria-current={n === current ? 'page' : undefined} onClick={() => onPageChange(n)}>{n}</Button>
      </span>)}
      <Button variant="outline" size="sm" className="h-7" aria-label={`下一页${buttonSuffix}`} disabled={current >= pages} onClick={() => onPageChange(current + 1)}>下一页</Button>
      <Select value={String(size)} onValueChange={v => onSizeChange(v === 'auto' ? 'auto' : Number(v))}>
        <SelectTrigger className="h-7 w-24 text-xs" aria-label={`${label}每页条数`}><SelectValue /></SelectTrigger>
        <SelectContent>
          {autoSize !== undefined && <SelectItem value="auto">自动({autoSize} 条/页)</SelectItem>}
          {[10, 15, 20, 30, 50].map(n => <SelectItem key={n} value={String(n)}>{n} 条/页</SelectItem>)}
        </SelectContent>
      </Select>
    </div>
  </nav>;
}
