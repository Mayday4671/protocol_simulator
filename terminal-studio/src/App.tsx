import { Toaster } from "sonner";
import { Sidebar } from "@/components/layout/Sidebar";
import { WorkspaceTabs } from "@/components/layout/WorkspaceTabs";
import { Dashboard } from "@/pages/Dashboard";
import { NodeManager } from "@/pages/NodeManager";
import { ProtocolManager } from "@/pages/ProtocolManager";
import { SimulationConsole } from "@/pages/SimulationConsole";
import { DictionaryManager } from "@/pages/DictionaryManager";
import { DeviceManager } from "@/pages/DeviceManager";
import { Monitor } from "@/pages/Monitor";
import { FlowManager } from "@/pages/FlowManager";
import { SkinProvider, useSkin } from "@/hooks/useSkin";

const workspacePages = [
  { path: "/", label: "总览", element: <Dashboard /> },
  { path: "/nodes", label: "通道管理", element: <NodeManager /> },
  { path: "/protocols", label: "协议管理", element: <ProtocolManager /> },
  { path: "/dict", label: "字典管理", element: <DictionaryManager /> },
  { path: "/devices", label: "设备管理", element: <DeviceManager /> },
  { path: "/flows", label: "业务流程", element: <FlowManager /> },
  { path: "/console", label: "交互控制台", element: <SimulationConsole /> },
  { path: "/monitor", label: "实时监控", element: <Monitor /> },
];

/** 全局背景层: 皮肤图片(可调不透明度) 或 环境光色斑氛围 */
function AppBackground() {
  const { url, skin } = useSkin();
  if (url) {
    return (
      <div
        className="fixed inset-0 z-0 bg-cover bg-center"
        style={{ backgroundImage: `url(${url})`, opacity: skin.opacity }}
      />
    );
  }
  return <div className="ambient-layer" />;
}

export default function App() {
  return (
    <SkinProvider>
      <div className="relative flex h-screen overflow-hidden">
        <AppBackground />
        <Sidebar />
        <div className="relative z-[1] flex min-w-0 flex-1"><WorkspaceTabs pages={workspacePages} /></div>
        <Toaster position="top-center" richColors closeButton />
      </div>
    </SkinProvider>
  );
}
