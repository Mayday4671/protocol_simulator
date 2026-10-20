import { useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";
import {
  ArrowDownToLine,
  Eraser,
  Pause,
  Play,
  Search,
  Trash2,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { clearLogs, listNodes } from "@/lib/api";
import { useUdpLogs } from "@/hooks/useUdpLogs";
import { DatagramDetail } from "@/components/monitor/DatagramDetail";
import { LogRow } from "@/components/monitor/LogRow";
import type { Node } from "@/types";

const MAX_RENDER = 500;

export function Monitor() {
  const { logs, paused, setPaused, clearScreen } = useUdpLogs();
  const [keyword, setKeyword] = useState("");
  const [direction, setDirection] = useState<string>("all");
  const [nodeFilter, setNodeFilter] = useState<string>("all");
  const [hexMode, setHexMode] = useState(true);
  const [autoScroll, setAutoScroll] = useState(true);
  const [nodes, setNodes] = useState<Node[]>([]);
  const [detailId, setDetailId] = useState<number | null>(null);
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    listNodes()
      .then(setNodes)
      .catch(() => {});
  }, []);

  const filtered = useMemo(() => {
    const kw = keyword.trim().toLowerCase();
    return logs.filter((l) => {
      if (direction !== "all" && l.direction !== direction) return false;
      if (nodeFilter !== "all" && String(l.node_id) !== nodeFilter)
        return false;
      if (!kw) return true;
      return (
        l.raw_hex.toLowerCase().includes(kw) ||
        l.raw_text.toLowerCase().includes(kw) ||
        (l.protocol_id ?? "").toLowerCase().includes(kw) ||
        l.remote_addr.toLowerCase().includes(kw) ||
        (l.parsed ?? "").toLowerCase().includes(kw)
      );
    });
  }, [logs, keyword, direction, nodeFilter]);

  const rendered = useMemo(
    () => filtered.slice(Math.max(0, filtered.length - MAX_RENDER)),
    [filtered],
  );

  useEffect(() => {
    if (autoScroll && !paused) {
      bottomRef.current?.scrollIntoView({ behavior: "auto" });
    }
  }, [rendered[rendered.length - 1]?.id, autoScroll, paused]);

  const handleClearDb = async () => {
    if (
      !window.confirm(
        "确定清理数据库中的历史日志吗? 将只保留最近 500 条（不影响当前屏幕显示）",
      )
    )
      return;
    try {
      const n = await clearLogs(500);
      toast.success(`已清理 ${n} 条历史日志`);
    } catch (e) {
      toast.error("清理失败", { description: String(e) });
    }
  };

  return (
    <div className="flex h-full flex-col p-6">
      {/* 过滤工具栏 */}
      <div className="mb-3 flex shrink-0 flex-wrap items-center gap-3">
        <div className="relative">
          <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
          <Input
            className="w-64 pl-8"
            aria-label="搜索报文、协议或地址"
            placeholder="关键字：报文 / 协议 / 地址"
            value={keyword}
            onChange={(e) => setKeyword(e.target.value)}
          />
        </div>
        <Select value={direction} onValueChange={setDirection}>
          <SelectTrigger aria-label="报文方向" className="w-28">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">全部方向</SelectItem>
            <SelectItem value="in">仅接收</SelectItem>
            <SelectItem value="out">仅发送</SelectItem>
          </SelectContent>
        </Select>
        <Select value={nodeFilter} onValueChange={setNodeFilter}>
          <SelectTrigger aria-label="监控通道" className="w-40">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">全部通道</SelectItem>
            {nodes.map((n) => (
              <SelectItem key={n.id} value={String(n.id)}>
                {n.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <div className="flex items-center gap-2">
          <Switch
            id="hex-toggle"
            checked={hexMode}
            onCheckedChange={setHexMode}
          />
          <Label htmlFor="hex-toggle" className="text-xs text-muted-foreground">
            {hexMode ? "十六进制显示" : "文本显示"}
          </Label>
        </div>
        <div className="flex items-center gap-2">
          <Switch
            id="scroll-toggle"
            checked={autoScroll}
            onCheckedChange={setAutoScroll}
          />
          <Label
            htmlFor="scroll-toggle"
            className="text-xs text-muted-foreground"
          >
            自动滚动
          </Label>
        </div>
        {!autoScroll && (
          <Button
            variant="ghost"
            size="icon"
            title="滚动到底部"
            onClick={() =>
              bottomRef.current?.scrollIntoView({ behavior: "smooth" })
            }
          >
            <ArrowDownToLine />
          </Button>
        )}
        <div className="ml-auto flex shrink-0 items-center gap-2">
          <Button
            variant={paused ? "default" : "outline"}
            size="sm"
            onClick={() => setPaused(!paused)}
          >
            {paused ? <Play /> : <Pause />}
            {paused ? "继续" : "暂停"}
          </Button>
          <Button variant="outline" size="sm" onClick={clearScreen}>
            <Eraser /> 清屏
          </Button>
          <Button variant="outline" size="sm" onClick={handleClearDb}>
            <Trash2 /> 清理历史
          </Button>
        </div>
      </div>

      {/* 日志终端: 颜色跟随主题 */}
      <div className="app-terminal flex min-h-0 flex-1 flex-col overflow-hidden rounded-xl border bg-muted/30 dark:bg-zinc-950">
        <div className="flex items-center gap-2 border-b border-border/60 px-3 py-2">
          <span className="h-2.5 w-2.5 rounded-full bg-red-500/80" />
          <span className="h-2.5 w-2.5 rounded-full bg-yellow-500/80" />
          <span className="h-2.5 w-2.5 rounded-full bg-green-500/80" />
          <span className="ml-2 font-mono text-xs text-muted-foreground">
            {paused ? "已暂停" : "实时"} · {rendered.length} 条
          </span>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto">
          {rendered.length === 0 ? (
            <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
              {paused
                ? "已暂停显示，继续后查看最新报文"
                : logs.length
                  ? "没有符合筛选条件的报文"
                  : "暂无报文"}
            </div>
          ) : (
            <>
              {rendered.map((log) => (
                <LogRow
                  key={log.id}
                  log={log}
                  hexMode={hexMode}
                  onOpen={(id) => setDetailId(id)}
                />
              ))}
              <div ref={bottomRef} />
            </>
          )}
        </div>
      </div>

      {/* 报文解析详情覆盖层 */}
      {detailId != null && (
        <DatagramDetail logId={detailId} onClose={() => setDetailId(null)} />
      )}
    </div>
  );
}
