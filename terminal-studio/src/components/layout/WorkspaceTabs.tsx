import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { FocusEvent, KeyboardEvent, ReactNode } from "react";
import { Route, Routes, useLocation, useNavigate } from "react-router-dom";
import { X } from "lucide-react";
import { WorkspaceActivityContext } from "@/components/layout/WorkspaceActivity";
import { cn } from "@/lib/utils";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { WindowControls } from "@/components/layout/WindowControls";
import { TabContextMenu, type TabCloseAction } from "@/components/layout/TabContextMenu";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription, DialogFooter } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { toast } from "sonner";

interface WorkspacePage {
  path: string;
  label: string;
  element: ReactNode;
}

type EditableInput = HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement;

function inputValue(input: EditableInput) {
  return input instanceof HTMLInputElement && /^(checkbox|radio)$/.test(input.type)
    ? String(input.checked)
    : input.value;
}

function WorkspacePanel({ page, active, registerPanel }: {
  page: WorkspacePage;
  active: boolean;
  registerPanel: (path: string, guard: () => boolean) => () => void;
}) {
  const closeGuards = useRef(new Set<() => boolean>());
  const inlineInputs = useRef(new Map<EditableInput, string>());
  const registerCloseGuard = useCallback((guard: () => boolean) => {
    closeGuards.current.add(guard);
    return () => { closeGuards.current.delete(guard); };
  }, []);
  useEffect(() => registerPanel(page.path, () =>
    [...closeGuards.current].some(guard => guard()) ||
    [...inlineInputs.current].some(([input, initial]) => inputValue(input) !== initial),
  ), [page.path, registerPanel]);
  const activity = useMemo(() => ({ active, inWorkspace: true, registerCloseGuard }), [active, registerCloseGuard]);

  const rememberInlineInput = (event: FocusEvent<HTMLElement>) => {
    // These pages have inline editors. Search/filter fields elsewhere are read-only navigation.
    if (page.path !== "/console" && page.path !== "/devices") return;
    const target = event.target;
    if (!(target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement)) return;
    if (target.closest('[role="dialog"]') || inlineInputs.current.has(target)) return;
    inlineInputs.current.set(target, inputValue(target));
  };

  return (
    <WorkspaceActivityContext.Provider value={activity}>
      <main
        id={`workspace-panel-${page.path.slice(1) || "home"}`}
        role="tabpanel"
        aria-labelledby={`workspace-tab-${page.path.slice(1) || "home"}`}
        className="app-main workspace-panel"
        hidden={!active}
        inert={!active}
        onFocusCapture={rememberInlineInput}
      >
        <Routes location={page.path}>
          <Route path={page.path} element={page.element} />
        </Routes>
      </main>
    </WorkspaceActivityContext.Provider>
  );
}

/** Each visited route owns a mounted page and its own scrolling surface until closed. */
export function WorkspaceTabs({ pages }: { pages: readonly WorkspacePage[] }) {
  const location = useLocation();
  const navigate = useNavigate();
  const activePath = pages.find(page => page.path === location.pathname)?.path ?? "/";
  const [openedPaths, setOpenedPaths] = useState<string[]>([activePath]);
  const paths = openedPaths.includes(activePath) ? openedPaths : [...openedPaths, activePath];
  const panelGuards = useRef(new Map<string, () => boolean>());
  const tabButtons = useRef(new Map<string, HTMLButtonElement>());
  const [pendingClose, setPendingClose] = useState<{ kind: 'window' } | { kind: 'tabs'; paths: string[]; labels: string[] } | null>(null);
  const [contextMenu, setContextMenu] = useState<{ path: string; x: number; y: number } | null>(null);
  const [closing, setClosing] = useState(false);

  useEffect(() => {
    setOpenedPaths(previous => previous.includes(activePath) ? previous : [...previous, activePath]);
    if (location.pathname !== activePath) navigate(activePath, { replace: true });
  }, [activePath, location.pathname, navigate]);
  const registerPanel = useCallback((path: string, guard: () => boolean) => {
    panelGuards.current.set(path, guard);
    return () => { panelGuards.current.delete(path); };
  }, []);
  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void getCurrentWindow().onCloseRequested(event => {
      if ([...panelGuards.current.values()].some(guard => guard())) {
        event.preventDefault();
        setPendingClose({ kind: 'window' });
      }
    }).then(off => { if (disposed) off(); else unlisten = off; }).catch(() => {});
    return () => { disposed = true; unlisten?.(); };
  }, []);

  const activate = (path: string, focus = false) => {
    if (path !== activePath) navigate(path);
    if (focus) tabButtons.current.get(path)?.focus();
  };
  const removeTabs = (targets: string[]) => {
    const removed = new Set(targets);
    const remaining = paths.filter(candidate => !removed.has(candidate));
    const index = paths.indexOf(activePath);
    const next = paths.slice(0, index).reverse().find(path => !removed.has(path)) ?? remaining[0] ?? "/";
    setOpenedPaths(remaining.length ? remaining : ["/"]);
    if (removed.has(activePath)) {
      navigate(next);
    }
    requestAnimationFrame(() => tabButtons.current.get(removed.has(activePath) ? next : activePath)?.focus());
  };
  const closeTabs = (targets: string[]) => {
    if (pendingClose || closing) return;
    const closingPaths = targets.filter(path => paths.includes(path) && !(paths.length === 1 && path === "/"));
    if (!closingPaths.length) return;
    const dirty = closingPaths.filter(path => panelGuards.current.get(path)?.());
    if (dirty.length) setPendingClose({ kind: 'tabs', paths: closingPaths, labels: dirty.map(path => pages.find(page => page.path === path)!.label) });
    else removeTabs(closingPaths);
  };
  const closeTargets = (path: string, action: TabCloseAction) => {
    const index = paths.indexOf(path);
    if (index < 0) return [];
    const targets = action === "current" ? [path] : action === "others" ? paths.filter(candidate => candidate !== path)
      : action === "left" ? paths.slice(0, index) : action === "right" ? paths.slice(index + 1) : paths;
    return targets.filter(candidate => !(paths.length === 1 && candidate === "/"));
  };
  const showContextMenu = (path: string, x?: number, y?: number) => {
    const rect = tabButtons.current.get(path)?.getBoundingClientRect();
    setContextMenu({ path, x: x ?? (rect?.left ?? 0), y: y ?? (rect?.bottom ?? 42) });
  };
  const confirmClose = async () => {
    if (!pendingClose || closing) return;
    if (pendingClose.kind === 'tabs') { removeTabs(pendingClose.paths); setPendingClose(null); return; }
    setClosing(true);
    try { await getCurrentWindow().destroy(); }
    catch (error) { toast.error(String(error)); setClosing(false); }
  };
  const onTabKeyDown = (event: KeyboardEvent<HTMLButtonElement>, path: string) => {
    if (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) {
      event.preventDefault(); showContextMenu(path); return;
    }
    const index = paths.indexOf(path);
    let next: string | undefined;
    if (event.key === "ArrowRight") next = paths[(index + 1) % paths.length];
    if (event.key === "ArrowLeft") next = paths[(index + paths.length - 1) % paths.length];
    if (event.key === "Home") next = paths[0];
    if (event.key === "End") next = paths[paths.length - 1];
    if (next) { event.preventDefault(); activate(next, true); }
    if (event.key === "Delete") { event.preventDefault(); closeTabs([path]); }
  };

  return (
    <div className="workspace flex min-h-0 min-w-0 flex-1 flex-col">
      <div className="workspace-titlebar">
      <div role="tablist" aria-label="工作区标签页" className="workspace-tabs">
        {paths.map(path => {
          const page = pages.find(candidate => candidate.path === path)!;
          const selected = path === activePath;
          return (
            <div key={path} role="presentation" className={cn("workspace-tab", selected && "workspace-tab-active")}
              onContextMenu={event => { event.preventDefault(); showContextMenu(path, event.clientX || undefined, event.clientY || undefined); }}>
              <button
                type="button"
                role="tab"
                id={`workspace-tab-${path.slice(1) || "home"}`}
                aria-controls={`workspace-panel-${path.slice(1) || "home"}`}
                aria-selected={selected}
                aria-haspopup="menu"
                tabIndex={selected ? 0 : -1}
                ref={element => { if (element) tabButtons.current.set(path, element); else tabButtons.current.delete(path); }}
                onClick={() => activate(path)}
                onKeyDown={event => onTabKeyDown(event, path)}
              >{page.label}</button>
              <button type="button" className="workspace-tab-close" aria-label={`关闭标签页：${page.label}`}
                disabled={paths.length === 1 && path === "/"}
                onClick={() => closeTabs([path])} title="关闭标签页"><X aria-hidden="true" className="h-3.5 w-3.5" /></button>
            </div>
          );
        })}
      </div>
      <div data-tauri-drag-region className="window-drag-region" aria-hidden="true" />
      <WindowControls />
      </div>
      {contextMenu && <TabContextMenu x={contextMenu.x} y={contextMenu.y}
        disabled={action => !closeTargets(contextMenu.path, action).length}
        onSelect={action => closeTabs(closeTargets(contextMenu.path, action))}
        onClose={() => setContextMenu(null)}
        returnFocus={() => tabButtons.current.get(contextMenu.path)?.focus()} />}
      <div className="workspace-pages">
        {paths.map(path => <WorkspacePanel key={path} page={pages.find(page => page.path === path)!} active={path === activePath} registerPanel={registerPanel} />)}
      </div>
      <Dialog open={pendingClose !== null} onOpenChange={open => { if (!open && !closing) setPendingClose(null); }}>
        <DialogContent className="max-w-sm" onCloseAutoFocus={event => {
          event.preventDefault();
          requestAnimationFrame(() => tabButtons.current.get(activePath)?.focus());
        }}>
          <DialogHeader>
            <DialogTitle>编辑尚未保存</DialogTitle>
            <DialogDescription>{pendingClose?.kind === 'tabs' ? pendingClose.labels.map(label => `“${label}”`).join('、') : '当前窗口'}有未保存的内容，关闭后将丢失。</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" disabled={closing} onClick={() => setPendingClose(null)}>继续编辑</Button>
            <Button variant="destructive" disabled={closing} onClick={() => void confirmClose()}>放弃并关闭</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
