import { useCallback, useEffect, useState } from "react";
import { Link } from "react-router-dom";
import {
  Activity,
  ArrowDownCircle,
  ArrowUpCircle,
  FileCode2,
  HeartPulse,
  Network,
  Radio,
} from "lucide-react";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { listNodes, listProtocols } from "@/lib/api";
import { useUdpLogs } from "@/hooks/useUdpLogs";
import type { Node, Protocol } from "@/types";
import { TrafficCharts } from "@/components/dashboard/TrafficCharts";

function StatCard({
  title,
  value,
  icon: Icon,
  hint,
}: {
  title: string;
  value: string | number;
  icon: React.ComponentType<{ className?: string }>;
  hint?: string;
}) {
  return (
    <Card>
      <CardHeader className="flex flex-row items-center justify-between space-y-0 px-5 pb-1 pt-3">
        <CardTitle className="text-sm font-medium">{title}</CardTitle>
        <Icon className="h-4 w-4 text-muted-foreground" />
      </CardHeader>
      <CardContent className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1 px-5 pb-3">
        <div className="text-2xl font-bold tabular-nums">{value}</div>
        {hint && <p className="text-xs text-muted-foreground">{hint}</p>}
      </CardContent>
    </Card>
  );
}

export function Dashboard() {
  const [nodes, setNodes] = useState<Node[]>([]);
  const [protocols, setProtocols] = useState<Protocol[]>([]);
  const { logs, stats, engine, refreshStats } = useUdpLogs();

  const refresh = useCallback(async () => {
    try {
      const [n, p] = await Promise.all([listNodes(), listProtocols()]);
      setNodes(n);
      setProtocols(p);
    } catch {
      /* dev 下后端未就绪忽略 */
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const listening = engine?.listening_nodes ?? [];
  const runningHb = engine?.running_heartbeats ?? [];
  const recent = logs.slice(-12).reverse();

  return (
    <div className="space-y-4 p-5">
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <StatCard
          title="通道数"
          value={nodes.length}
          icon={Network}
          hint={`${listening.length} 个监听中`}
        />
        <StatCard
          title="协议数"
          value={protocols.length}
          icon={FileCode2}
        />
        <StatCard
          title="今日接收"
          value={stats.today_in}
          icon={ArrowDownCircle}
          hint={`历史共 ${stats.total} 条`}
        />
        <StatCard
          title="今日发送"
          value={stats.today_out}
          icon={ArrowUpCircle}
          hint={`${runningHb.length} 个心跳运行中`}
        />
      </div>

      <div className="grid gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader className="flex-row items-center justify-between space-y-0 px-5 py-4">
            <div className="space-y-1.5">
              <CardTitle className="flex items-center gap-2 text-base">
                <Radio className="h-4 w-4" /> 通道状态
              </CardTitle>
            </div>
            <Button asChild variant="outline" size="sm">
              <Link to="/nodes">管理通道</Link>
            </Button>
          </CardHeader>
          <CardContent className="space-y-2 px-5 pb-4">
            {nodes.length === 0 && (
              <p className="py-6 text-center text-sm text-muted-foreground">
                暂无通道
              </p>
            )}
            {nodes.map((n) => {
              const isListening = listening.includes(n.id);
              return (
                <div
                  key={n.id}
                  className="flex items-center justify-between rounded-lg border px-3 py-2"
                >
                  <div>
                    <span className="text-sm font-medium">{n.name}</span>
                    <span className="ml-2 font-mono text-xs text-muted-foreground">
                      {n.local_ip}:{n.local_port} → {n.remote_ip}:{n.remote_port}
                    </span>
                  </div>
                  {isListening ? (
                    <Badge className="gap-1 bg-primary text-primary-foreground">
                      <Activity className="h-3 w-3" /> 监听中
                    </Badge>
                  ) : (
                    <Badge variant="outline">{n.role === "simulator" ? "模拟端" : "目标端"}</Badge>
                  )}
                </div>
              );
            })}
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex-row items-center justify-between space-y-0 px-5 py-4">
            <div className="space-y-1.5">
              <CardTitle className="flex items-center gap-2 text-base">
                <HeartPulse className="h-4 w-4" /> 最近报文
              </CardTitle>
            </div>
            <Button asChild variant="outline" size="sm">
              <Link to="/monitor">打开监控</Link>
            </Button>
          </CardHeader>
          <CardContent className="space-y-1.5 px-5 pb-4">
            {recent.length === 0 && (
              <p className="py-6 text-center text-sm text-muted-foreground">
                暂无通信记录
              </p>
            )}
            {recent.map((l) => (
              <div key={l.id} className="flex items-center gap-2 font-mono text-xs">
                <span className="text-muted-foreground">{l.ts.slice(11)}</span>
                <Badge
                  className={
                    l.direction === "in"
                      ? "bg-sky-600 hover:bg-sky-600"
                      : "bg-emerald-600 hover:bg-emerald-600"
                  }
                >
                  {l.direction === "in" ? "收" : "发"}
                </Badge>
                <span className="truncate text-muted-foreground">
                  {l.raw_hex || "-"}
                </span>
              </div>
            ))}
          </CardContent>
        </Card>
      </div>
      <TrafficCharts onUpdated={refreshStats} />
    </div>
  );
}
