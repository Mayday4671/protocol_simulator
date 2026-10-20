import { useRef } from "react";
import { CopyX, PanelLeftClose, PanelRightClose, PanelTop, X, type LucideIcon } from "lucide-react";
import { Popover, PopoverAnchor, PopoverContent } from "@/components/ui/popover";

export type TabCloseAction = "current" | "others" | "left" | "right" | "all";
export const TAB_CLOSE_ACTIONS: { action: TabCloseAction; label: string; icon: LucideIcon }[] = [
  { action: "current", label: "关闭当前", icon: X },
  { action: "others", label: "关闭其他", icon: PanelTop },
  { action: "left", label: "关闭左侧", icon: PanelLeftClose },
  { action: "right", label: "关闭右侧", icon: PanelRightClose },
  { action: "all", label: "关闭全部", icon: CopyX },
];

export function TabContextMenu({ x, y, disabled, onSelect, onClose, returnFocus }: {
  x: number;
  y: number;
  disabled: (action: TabCloseAction) => boolean;
  onSelect: (action: TabCloseAction) => void;
  onClose: () => void;
  returnFocus: () => void;
}) {
  const menuRef = useRef<HTMLDivElement>(null);
  const restoreFocus = useRef(true);
  const enabledItems = () => [...(menuRef.current?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled)') ?? [])];
  return <Popover open onOpenChange={open => { if (!open) onClose(); }}>
    <PopoverAnchor asChild>
      <span aria-hidden="true" className="pointer-events-none fixed h-0 w-0" style={{ left: x, top: y }} />
    </PopoverAnchor>
    <PopoverContent ref={menuRef} role="menu" aria-label="标签页操作" align="start" sideOffset={2} collisionPadding={8}
      className="w-32 p-1"
      onContextMenu={event => event.preventDefault()}
      onOpenAutoFocus={event => { event.preventDefault(); (enabledItems()[0] ?? menuRef.current)?.focus(); }}
      onCloseAutoFocus={event => { event.preventDefault(); if (restoreFocus.current) returnFocus(); }}
      onInteractOutside={() => { restoreFocus.current = false; }}
      onKeyDown={event => {
        if (event.key === "Tab") { event.preventDefault(); onClose(); return; }
        if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
        event.preventDefault();
        const items = enabledItems();
        const index = items.indexOf(document.activeElement as HTMLButtonElement);
        const next = event.key === "Home" ? 0 : event.key === "End" ? items.length - 1
          : (index + (event.key === "ArrowUp" ? -1 : 1) + items.length) % items.length;
        items[next]?.focus();
      }}>
      {TAB_CLOSE_ACTIONS.map(({ action, label, icon: Icon }) => <div key={action}>
        {action === "all" && <div role="separator" className="my-1 border-t" />}
        <button type="button" role="menuitem" tabIndex={-1} disabled={disabled(action)}
          className="flex h-7 w-full items-center gap-2 rounded-sm px-2 text-left text-xs hover:bg-accent focus:bg-accent focus:outline-none disabled:pointer-events-none disabled:opacity-40"
          onClick={() => { restoreFocus.current = false; onSelect(action); onClose(); }}>
          <Icon aria-hidden="true" className="h-3.5 w-3.5 shrink-0 opacity-70" />
          <span className="whitespace-nowrap">{label}</span>
        </button>
      </div>)}
    </PopoverContent>
  </Popover>;
}
