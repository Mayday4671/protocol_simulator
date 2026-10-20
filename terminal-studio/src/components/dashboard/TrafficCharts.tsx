import { useContext, useEffect, useState } from "react";
import { RefreshCw } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { WorkspaceActivityContext } from "@/components/layout/WorkspaceActivity";
import { cn } from "@/lib/utils";
import "./TrafficCharts.css";

type Range = "1h" | "24h" | "7d";
interface Count { received: number; sent: number }
interface Bucket extends Count { start: string; end: string }
interface TrafficStats extends Count {
  start: string;
  end: string;
  buckets: Bucket[];
  channels: (Count & { node_id: number | null; name: string })[];
}
const ranges: [Range, string][] = [["1h", "最近 1 小时"], ["24h", "最近 24 小时"], ["7d", "最近 7 天"]];
const number = (n: number) => n.toLocaleString("zh-CN");

function Legend() {
  return <div className="flex shrink-0 items-center gap-4 text-xs text-muted-foreground">
    <span className="flex items-center gap-1.5"><i className="traffic-dot traffic-received" />接收</span>
    <span className="flex items-center gap-1.5"><i className="traffic-dot traffic-sent" />发送</span>
  </div>;
}

function Trend({ data, range }: { data: TrafficStats; range: Range }) {
  const [selected, setSelected] = useState<number | null>(null);
  const buckets = data.buckets;
  const max = Math.max(2, Math.ceil(Math.max(...buckets.flatMap(b => [b.received, b.sent])) / 2) * 2);
  const left = 46, right = 684, top = 18, bottom = 176;
  const x = (i: number) => left + i * (right - left) / Math.max(1, buckets.length - 1);
  const y = (v: number) => bottom - v / max * (bottom - top);
  const points = (key: keyof Count) => buckets.map((b, i) => `${x(i)},${y(b[key])}`).join(" ");
  const label = (date: string) => range === "7d" ? date.slice(5, 10) : date.slice(11, 16);
  const current = selected == null ? undefined : buckets[selected];
  const tickIndices = [...new Set([0, Math.round((buckets.length - 1) / 3), Math.round((buckets.length - 1) * 2 / 3), buckets.length - 1])];
  return <div>
    <svg viewBox="0 0 708 211" className="traffic-trend" role="group" aria-label="收发报文趋势图" onMouseLeave={() => setSelected(null)}>
      {[0, max / 2, max].map(value => <g key={value}>
        <line x1={left} x2={right} y1={y(value)} y2={y(value)} className="traffic-grid" />
        <text x={left - 8} y={y(value) + 4} textAnchor="end" className="traffic-axis">{number(value)}</text>
      </g>)}
      {tickIndices.map(i => <text key={i} x={x(i)} y={201} textAnchor={i === 0 ? "start" : i === buckets.length - 1 ? "end" : "middle"} className="traffic-axis">{label(buckets[i].start)}</text>)}
      <polyline points={points("received")} className="traffic-line traffic-line-received" />
      <polyline points={points("sent")} className="traffic-line traffic-line-sent" />
      {current && selected != null && <g aria-hidden="true">
        <line x1={x(selected)} x2={x(selected)} y1={top} y2={bottom} className="traffic-guide" />
        <circle cx={x(selected)} cy={y(current.received)} r={4} className="traffic-received" />
        <circle cx={x(selected)} cy={y(current.sent)} r={4} className="traffic-sent" />
      </g>}
      {buckets.map((bucket, i) => {
        const a = i === 0 ? left : (x(i - 1) + x(i)) / 2;
        const b = i === buckets.length - 1 ? right : (x(i) + x(i + 1)) / 2;
        return <rect key={i} x={a} y={top} width={b - a} height={bottom - top} fill="transparent" tabIndex={0}
          role="img" aria-label={`${bucket.start} 至 ${bucket.end}，接收 ${bucket.received} 条，发送 ${bucket.sent} 条`}
          onMouseEnter={() => setSelected(i)} onFocus={() => setSelected(i)} onBlur={() => setSelected(null)} className="traffic-hit" />;
      })}
    </svg>
    <div className="min-h-9 pt-2 text-xs text-muted-foreground" aria-live="polite">
      {current ? <div className="flex flex-wrap gap-x-4 gap-y-1">
        <span>{current.start.slice(5)} — {current.end.slice(5)}</span>
        <span className="traffic-received-text">接收 {number(current.received)} 条</span>
        <span className="traffic-sent-text">发送 {number(current.sent)} 条</span>
      </div> : data.received + data.sent === 0 ? "所选时段暂无报文" : <span>每{range === "1h" ? " 5 分钟" : range === "24h" ? "小时" : "天"}报文数 · 单位：条</span>}
    </div>
  </div>;
}

export function TrafficCharts({ onUpdated }: { onUpdated?: () => Promise<void> }) {
  const { active } = useContext(WorkspaceActivityContext);
  const [range, setRange] = useState<Range>("24h");
  const [data, setData] = useState<TrafficStats | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [refresh, setRefresh] = useState(0);

  useEffect(() => {
    if (!active) return;
    let cancelled = false, inFlight = false;
    const load = async () => {
      if (inFlight || document.hidden) return;
      inFlight = true;
      setBusy(true);
      try {
        const result = await invoke<TrafficStats>("traffic_stats", { range });
        if (!cancelled) {
          setData(result);
          setError("");
          void onUpdated?.();
        }
      } catch (e) {
        if (!cancelled) setError(String(e));
      } finally {
        inFlight = false;
        if (!cancelled) setBusy(false);
      }
    };
    void load();
    const timer = window.setInterval(() => void load(), 5000);
    const visible = () => { if (!document.hidden) void load(); };
    document.addEventListener("visibilitychange", visible);
    return () => { cancelled = true; clearInterval(timer); document.removeEventListener("visibilitychange", visible); };
  }, [active, range, refresh, onUpdated]);

  const total = data ? data.received + data.sent : 0;
  const channelMax = Math.max(1, ...(data?.channels.map(c => c.received + c.sent) ?? []));
  return <section className="traffic-charts space-y-3" aria-label="通信统计">
    <div className="flex flex-wrap items-center justify-between gap-3">
      <div role="group" aria-label="统计时段" className="flex flex-wrap gap-1 rounded-lg border bg-card p-1">
        {ranges.map(([key, label]) => <button key={key} type="button" aria-pressed={range === key}
          className={cn("rounded-md px-3 py-1.5 text-xs transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring", range === key ? "bg-primary text-primary-foreground" : "text-muted-foreground hover:bg-accent")}
          onClick={() => { if (range !== key) { setRange(key); setData(null); setError(""); } }}>{label}</button>)}
      </div>
      <div className="flex items-center gap-3 text-xs text-muted-foreground">
        {data && <span title={`统计区间：${data.start} 至 ${data.end}`}>更新于 {data.end.slice(11, 19)}</span>}
        <Button variant="outline" size="sm" aria-label="刷新通信统计" disabled={busy} onClick={() => setRefresh(n => n + 1)}>
          <RefreshCw className={cn("h-3.5 w-3.5", busy && "animate-spin")} />刷新
        </Button>
      </div>
    </div>
    {error && <div role="alert" className="text-sm text-destructive">{data ? "统计更新失败，当前显示上次结果。" : "无法读取通信统计。"}<button className="ml-2 underline" onClick={() => setRefresh(n => n + 1)}>重试</button></div>}
    <div className="grid gap-6 xl:grid-cols-[minmax(0,1.5fr)_minmax(0,1fr)]">
      <Card className="min-w-0">
        <CardHeader className="flex-row flex-wrap items-center justify-between gap-3 space-y-0 pb-3">
          <CardTitle className="text-base">收发趋势</CardTitle><Legend />
        </CardHeader>
        <CardContent>
          <div className="mb-3 flex flex-wrap items-baseline gap-x-5 gap-y-1 text-xs text-muted-foreground">
            <span>接收 <strong className="ml-1 text-lg font-semibold tabular-nums text-foreground">{data ? number(data.received) : "—"}</strong> 条</span>
            <span>发送 <strong className="ml-1 text-lg font-semibold tabular-nums text-foreground">{data ? number(data.sent) : "—"}</strong> 条</span>
          </div>
          {data ? <Trend key={range} data={data} range={range} /> : <div className="flex h-56 items-center justify-center text-sm text-muted-foreground">{error ? "统计不可用" : "正在读取统计…"}</div>}
        </CardContent>
      </Card>
      <Card className="min-w-0">
        <CardHeader className="flex-row items-center justify-between space-y-0 pb-3">
          <CardTitle className="text-base">各通道报文量</CardTitle>
          <span className="text-xs text-muted-foreground">{data ? `共 ${number(total)} 条` : "—"}</span>
        </CardHeader>
        <CardContent>
          <div className="mb-4"><Legend /></div>
          <div className="max-h-60 min-h-52 space-y-4 overflow-y-auto pr-1" aria-label="通道报文量排行">
            {data?.channels.map(channel => <div key={channel.node_id ?? "none"}>
              <div className="mb-1.5 flex items-center justify-between gap-3 text-sm">
                <span className="truncate" title={channel.name}>{channel.name}</span>
                <span className="shrink-0 text-xs tabular-nums text-muted-foreground">{number(channel.received + channel.sent)} 条</span>
              </div>
              <div role="img" aria-label={`${channel.name}：接收 ${channel.received} 条，发送 ${channel.sent} 条`} title={`接收 ${number(channel.received)} 条 / 发送 ${number(channel.sent)} 条`} className="flex h-2.5 overflow-hidden rounded-sm bg-muted">
                <div className="traffic-received" style={{ width: `${channel.received / channelMax * 100}%` }} />
                <div className="traffic-sent" style={{ width: `${channel.sent / channelMax * 100}%` }} />
              </div>
            </div>)}
            {(!data || !data.channels.length) && <div className="flex min-h-48 items-center justify-center text-sm text-muted-foreground">{!data ? error ? "统计不可用" : "正在读取统计…" : "所选时段暂无报文"}</div>}
          </div>
        </CardContent>
      </Card>
    </div>
  </section>;
}
