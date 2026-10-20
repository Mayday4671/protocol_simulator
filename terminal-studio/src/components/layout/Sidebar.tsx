import { useEffect, useState } from "react";
import { NavLink } from "react-router-dom";
import {
  LayoutDashboard,
  BookMarked,
  Network,
  FileCode2,
  HardDrive,
  Radio, Workflow,
  Terminal,
  Sun,
  Moon,
  Palette,
} from "lucide-react";
import { SkinPanel } from "@/components/layout/SkinPanel";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";

const navItems = [
  { to: "/", label: "总览", icon: LayoutDashboard },
  { to: "/nodes", label: "通道管理", icon: Network },
  { to: "/protocols", label: "协议管理", icon: FileCode2 },
  { to: "/dict", label: "字典管理", icon: BookMarked },
  { to: "/devices", label: "设备管理", icon: HardDrive },
  { to: "/flows", label: "业务流程", icon: Workflow },
  { to: "/console", label: "交互控制台", icon: Radio },
  { to: "/monitor", label: "实时监控", icon: Terminal },
];

function SkinButton() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <Button
        variant="ghost"
        size="icon"
        onClick={() => setOpen(true)}
        title="皮肤设置（背景图片与透明度）"
      >
        <Palette />
      </Button>
      <SkinPanel open={open} onOpenChange={setOpen} />
    </>
  );
}

function ThemeToggle() {
  const [dark, setDark] = useState(
    () => localStorage.getItem("theme") !== "light",
  );
  useEffect(() => {
    document.documentElement.classList.toggle("dark", dark);
    localStorage.setItem("theme", dark ? "dark" : "light");
  }, [dark]);
  return (
    <Button
      variant="ghost"
      size="icon"
      onClick={() => setDark((v) => !v)}
      title={dark ? "切换到明亮模式" : "切换到暗黑模式"}
    >
      {dark ? <Sun /> : <Moon />}
    </Button>
  );
}

export function Sidebar() {
  return (
    <aside className="app-sidebar relative z-[1] flex h-full w-56 flex-col border-r bg-card">
      <div data-tauri-drag-region className="flex h-[42px] shrink-0 select-none items-center gap-2.5 border-b px-4">
        <img data-tauri-drag-region src="/app-icon.png" alt="" className="h-6 w-6 rounded-md" draggable={false} />
        <div data-tauri-drag-region className="leading-tight">
          <div data-tauri-drag-region className="text-sm font-semibold">多协议模拟系统</div>
        </div>
      </div>

      <nav className="flex-1 space-y-1 px-3 pt-4">
        {navItems.map(({ to, label, icon: Icon }) => (
          <NavLink
            key={to}
            to={to}
            end={to === "/"}
            className={({ isActive }) =>
              cn(
                "flex items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors",
                isActive
                  ? "bg-primary/15 text-primary"
                  : "text-muted-foreground hover:bg-accent hover:text-accent-foreground",
              )
            }
          >
            <Icon className="h-4 w-4" />
            {label}
          </NavLink>
        ))}
      </nav>

      <div className="flex items-center justify-between border-t px-3 py-3">
        <span className="px-2 text-xs text-muted-foreground">v0.1.0</span>
        <div className="flex items-center gap-0.5">
          <SkinButton />
          <ThemeToggle />
        </div>
      </div>
    </aside>
  );
}
