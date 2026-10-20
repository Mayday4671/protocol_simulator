// 全局日志/引擎状态上下文：订阅 Tauri 事件, 维护前端日志缓冲
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { engineStatus as fetchEngineStatus, logStats } from "@/lib/api";
import type {
  DeviceSavedEvent,
  EngineStatus,
  LoginSuccessEvent,
  LogStats,
  SessionUpdatedEvent,
  UdpLogEvent,
} from "@/types";

const MAX_LOGS = 3000;

interface LogContextValue {
  logs: UdpLogEvent[];
  paused: boolean;
  setPaused: (v: boolean) => void;
  clearScreen: () => void;
  stats: LogStats;
  refreshStats: () => Promise<void>;
  engine: EngineStatus | null;
  refreshEngine: () => Promise<void>;
}

const LogContext = createContext<LogContextValue | null>(null);

export function LogProvider({ children }: { children: ReactNode }) {
  const [logs, setLogs] = useState<UdpLogEvent[]>([]);
  const [paused, setPaused] = useState(false);
  const [stats, setStats] = useState<LogStats>({
    today_in: 0,
    today_out: 0,
    total: 0,
  });
  const [engine, setEngine] = useState<EngineStatus | null>(null);

  const pausedRef = useRef(paused);
  pausedRef.current = paused;
  const lastStatsRefresh = useRef(0);

  const refreshStats = useCallback(async () => {
    try {
      setStats(await logStats());
    } catch {
      /* dev 环境后端未就绪时忽略 */
    }
  }, []);

  const refreshEngine = useCallback(async () => {
    try {
      setEngine(await fetchEngineStatus());
    } catch {
      /* ignore */
    }
  }, []);

  useEffect(() => {
    // listen 是异步注册的：若组件在注册完成前被卸载（StrictMode/HMR），
    // 必须在注册完成后立刻反注册，否则监听器泄漏导致日志重复
    let cancelled = false;
    const unlisten: UnlistenFn[] = [];

    const setup = async () => {
      const fns = await Promise.all([
        listen<UdpLogEvent>("udp-log", (e) => {
          if (pausedRef.current) return;
          setLogs((prev) => {
            const next = [...prev, e.payload];
            return next.length > MAX_LOGS
              ? next.slice(next.length - MAX_LOGS)
              : next;
          });
          const now = Date.now();
          if (now - lastStatsRefresh.current > 2000) {
            lastStatsRefresh.current = now;
            void refreshStats();
          }
        }),
        listen<EngineStatus>("engine-status", (e) => {
          setEngine(e.payload);
        }),
        listen<LoginSuccessEvent>("login-success", (e) => {
          const { node_name, protocol_name } = e.payload;
          toast.success(`节点「${node_name}」入网成功`, {
            description: `收到应答「${protocol_name}」, auto_start 心跳已自动启动`,
          });
        }),
        listen<DeviceSavedEvent>("device-saved", (e) => {
          const { op_label, count, devices, protocol_name } = e.payload;
          toast.success(`已${op_label} ${count} 台设备`, {
            description: `${protocol_name} 配置成功, ${op_label}: ${devices.join("、")}`,
          });
        }),
        listen<SessionUpdatedEvent>("session-updated", (e) => {
          const { node_name, protocol_name, session_hex, device } = e.payload;
          toast.success(`终端「${device ?? node_name}」会话已更新`, {
            description: `${protocol_name} 返回 Session 0x${session_hex}, 后续报文将自动携带`,
          });
        }),
      ]);
      if (cancelled) {
        fns.forEach((fn) => fn());
        return;
      }
      unlisten.push(...fns);
    };
    void setup();
    void refreshStats();
    void refreshEngine();

    return () => {
      cancelled = true;
      unlisten.forEach((fn) => fn());
    };
  }, [refreshStats, refreshEngine]);

  const clearScreen = useCallback(() => setLogs([]), []);

  return (
    <LogContext.Provider
      value={{
        logs,
        paused,
        setPaused,
        clearScreen,
        stats,
        refreshStats,
        engine,
        refreshEngine,
      }}
    >
      {children}
    </LogContext.Provider>
  );
}

export function useUdpLogs() {
  const ctx = useContext(LogContext);
  if (!ctx) throw new Error("useUdpLogs 必须在 LogProvider 内使用");
  return ctx;
}
