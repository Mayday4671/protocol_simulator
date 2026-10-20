import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";
import type { Direction, UdpLogEvent } from "@/types";

export function directionBadge(d: Direction) {
  return d === "in" ? (
    <Badge className="bg-sky-600 hover:bg-sky-600">收</Badge>
  ) : (
    <Badge className="bg-emerald-600 hover:bg-emerald-600">发</Badge>
  );
}

export function LogRow({
  log,
  hexMode,
  onOpen,
}: {
  log: UdpLogEvent;
  hexMode: boolean;
  onOpen: (id: number) => void;
}) {
  const payload = hexMode || !log.raw_text ? log.raw_hex : log.raw_text;
  const isError = log.status === "error";
  return (
    <div
      className={cn(
        "log-row flex cursor-pointer items-center gap-3 border-b border-border/40 px-3 py-1.5 font-mono text-xs",
        isError && "bg-destructive/10",
        log.direction === "in"
          ? "hover:bg-sky-500/5"
          : "hover:bg-emerald-500/5",
      )}
      role="button"
      tabIndex={0}
      aria-label={`查看报文详情：${log.protocol_name ?? "未识别"} ${log.ts}`}
      title="点击查看报文详情"
      onClick={() => onOpen(log.id)}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onOpen(log.id);
        }
      }}
    >
      <span className="w-44 shrink-0 text-muted-foreground">{log.ts}</span>
      <span className="w-12 shrink-0">{directionBadge(log.direction)}</span>
      <span className="w-28 shrink-0 truncate" title={log.node_name ?? ""}>
        {log.node_name ?? "-"}
      </span>
      <span
        className="w-44 shrink-0 truncate text-muted-foreground"
        title={log.remote_addr}
      >
        {log.remote_addr}
      </span>
      <span
        className="w-40 shrink-0 truncate font-sans text-xs font-medium"
        title={log.protocol_name ?? ""}
      >
        {log.protocol_name ? (
          <span className="text-primary">{log.protocol_name}</span>
        ) : (
          <span className="text-muted-foreground/60">未识别</span>
        )}
      </span>
      <span
        className={cn(
          "min-w-0 flex-1 truncate",
          isError ? "text-red-600 dark:text-red-400" : "text-foreground/90",
        )}
        title={payload}
      >
        {payload}
      </span>
      {log.parsed && (
        <span
          className="max-w-64 shrink-0 truncate text-muted-foreground"
          title={log.parsed}
        >
          {log.parsed}
        </span>
      )}
    </div>
  );
}
