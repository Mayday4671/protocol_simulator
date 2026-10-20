// 皮肤 + 主题色: 本地背景图(可调透明度) 与 强调色切换, localStorage 持久化
import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";

export interface SkinState {
  /** 本地图片绝对路径; null = 未启用皮肤 */
  path: string | null;
  /** 背景图不透明度 0-1 */
  opacity: number;
}

/** 强调色预设: HSL 三元组与配套前景色(亮色用深字/深色用白字) */
export interface AccentPreset {
  id: string;
  name: string;
  /** H S L */
  hsl: [number, number, number];
  /** 亮色模式下的按钮文字色 */
  fgLight: string;
  /** 深色模式下的按钮文字色 */
  fgDark: string;
}

export const ACCENTS: AccentPreset[] = [
  { id: "graphite", name: "石墨", hsl: [240, 6, 10], fgLight: "0 0% 98%", fgDark: "0 0% 98%" },
  { id: "azure", name: "极光蓝", hsl: [217, 91, 55], fgLight: "0 0% 100%", fgDark: "0 0% 100%" },
  { id: "sky", name: "晴空蓝", hsl: [199, 89, 46], fgLight: "0 0% 100%", fgDark: "0 0% 100%" },
  { id: "cyan", name: "青碧", hsl: [187, 80, 38], fgLight: "0 0% 100%", fgDark: "0 0% 100%" },
  { id: "teal", name: "松石绿", hsl: [173, 75, 36], fgLight: "0 0% 100%", fgDark: "0 0% 100%" },
  { id: "green", name: "翠绿", hsl: [142, 68, 38], fgLight: "0 0% 100%", fgDark: "0 0% 100%" },
  { id: "amber", name: "琥珀", hsl: [38, 92, 46], fgLight: "30 40% 12%", fgDark: "30 45% 10%" },
  { id: "orange", name: "暖橙", hsl: [24, 90, 50], fgLight: "24 45% 10%", fgDark: "0 0% 100%" },
  { id: "red", name: "绯红", hsl: [0, 78, 55], fgLight: "0 0% 100%", fgDark: "0 0% 100%" },
  { id: "rose", name: "玫红", hsl: [346, 77, 52], fgLight: "0 0% 100%", fgDark: "0 0% 100%" },
  { id: "violet", name: "紫罗兰", hsl: [262, 80, 58], fgLight: "0 0% 100%", fgDark: "0 0% 100%" },
  { id: "indigo", name: "靛蓝", hsl: [239, 80, 62], fgLight: "0 0% 100%", fgDark: "0 0% 100%" },
];

const DEFAULT_SKIN: SkinState = { path: null, opacity: 0.35 };
const KEY_SKIN = "app-skin";
const KEY_ACCENT = "app-accent";

interface SkinCtx {
  skin: SkinState;
  setSkin: (path: string | null, opacity?: number) => void;
  setOpacity: (v: number) => void;
  /** webview 可用的图片 URL（asset 协议） */
  url: string | null;
  /** 当前强调色 id */
  accentId: string;
  setAccent: (id: string) => void;
}

const Ctx = createContext<SkinCtx | null>(null);

function loadSkin(): SkinState {
  try {
    const raw = localStorage.getItem(KEY_SKIN);
    if (raw) return { ...DEFAULT_SKIN, ...(JSON.parse(raw) as Partial<SkinState>) };
  } catch {
    /* ignore */
  }
  return DEFAULT_SKIN;
}

/** 强调色写入 CSS 变量: primary 与 ring 联动, 前景色按明暗模式取预设 */
/**
 * 主题色写入整套联动 CSS 变量 —— 切色零残留:
 * primary(按钮/选中) / ring(焦点) / accent(hover悬浮) / secondary(徽章/Tabs底)
 * 及各自前景色, 按明暗模式取合适明度
 */
function applyAccent(id: string, dark: boolean) {
  const preset = ACCENTS.find((a) => a.id === id) ?? ACCENTS[0];
  const root = document.documentElement;
  const [h, s, l] = preset.hsl;
  root.style.setProperty("--primary", `${h} ${s}% ${l}%`);
  root.style.setProperty("--ring", `${h} ${s}% ${Math.min(l + 8, 65)}%`);
  root.style.setProperty(
    "--primary-foreground",
    dark ? preset.fgDark : preset.fgLight,
  );
  if (s <= 8) {
    // 无彩色主题(石墨): 清除内联覆盖, 交还给主题 CSS 的明/暗默认值
    // (否则亮色时写入的近黑 primary 在暗色下变成近黑文字, 隐形)
    for (const v of [
      "--primary",
      "--ring",
      "--primary-foreground",
      "--accent",
      "--accent-foreground",
      "--secondary",
      "--secondary-foreground",
    ]) {
      root.style.removeProperty(v);
    }
    return;
  }
  // hover/悬浮底: 浅色=极浅主题色, 深色=深主题色调; 前景为主题色深/浅版
  root.style.setProperty(
    "--accent",
    dark ? `${h} ${Math.min(s, 45)}% 20%` : `${h} ${Math.min(s, 70)}% 95%`,
  );
  root.style.setProperty(
    "--accent-foreground",
    dark ? `${h} ${Math.min(s, 80)}% 82%` : `${h} ${Math.min(s, 90)}% 26%`,
  );
  // 徽章/Tab 选中底: 主题色淡底 + 主题色前景
  root.style.setProperty(
    "--secondary",
    dark ? `${h} ${Math.min(s, 35)}% 26%` : `${h} ${Math.min(s, 60)}% 93%`,
  );
  root.style.setProperty(
    "--secondary-foreground",
    dark ? `${h} ${Math.min(s, 85)}% 78%` : `${h} ${Math.min(s, 95)}% 32%`,
  );
}

export function SkinProvider({ children }: { children: ReactNode }) {
  const [skin, setSkinState] = useState<SkinState>(loadSkin);
  const [accentId, setAccentId] = useState(
    () => localStorage.getItem(KEY_ACCENT) ?? "graphite",
  );

  useEffect(() => {
    localStorage.setItem(KEY_SKIN, JSON.stringify(skin));
  }, [skin]);

  // 强调色: 立即应用并持久化; 跟随明暗模式切换前景色
  useEffect(() => {
    localStorage.setItem(KEY_ACCENT, accentId);
    const dark = document.documentElement.classList.contains("dark");
    applyAccent(accentId, dark);
    // 明暗切换时重算前景色（ThemeToggle 只切 class, 这里补听）
    const ob = new MutationObserver(() => {
      applyAccent(accentId, document.documentElement.classList.contains("dark"));
    });
    ob.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["class"],
    });
    return () => ob.disconnect();
  }, [accentId]);

  // 皮肤激活时给根节点打标; 透明度滑杆驱动整套联动变量:
  // 滑杆越高背景越浓、界面越通透(露图越多), 越低背景越淡、界面越实(可读性越好)
  useEffect(() => {
    const root = document.documentElement;
    root.classList.toggle("skin-on", skin.path != null);
    const v = skin.opacity; // 0.05 - 1
    const card = Math.min(0.98, Math.max(0.55, 1 - 0.42 * v));
    root.style.setProperty("--skin-card", String(card));
    root.style.setProperty("--skin-sidebar", String(Math.min(0.98, card + 0.08)));
    root.style.setProperty("--skin-popover", String(Math.min(0.97, card + 0.15)));
  }, [skin.path, skin.opacity]);

  const setSkin = useCallback((path: string | null, opacity?: number) => {
    setSkinState((s) => ({ path, opacity: opacity ?? s.opacity }));
  }, []);
  const setOpacity = useCallback((v: number) => {
    setSkinState((s) => ({ ...s, opacity: v }));
  }, []);
  const setAccent = useCallback((id: string) => setAccentId(id), []);

  const url = useMemo(
    () => (skin.path ? convertFileSrc(skin.path) : null),
    [skin.path],
  );

  const value = useMemo(
    () => ({ skin, setSkin, setOpacity, url, accentId, setAccent }),
    [skin, setSkin, setOpacity, url, accentId, setAccent],
  );
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useSkin() {
  const c = useContext(Ctx);
  if (!c) throw new Error("useSkin 必须在 SkinProvider 内使用");
  return c;
}
