import { createContext, useContext, useEffect, useRef, useState } from "react";

export const WorkspaceActivityContext = createContext<{
  active: boolean;
  inWorkspace?: boolean;
  registerCloseGuard: (guard: () => boolean) => () => void;
}>({ active: true, registerCloseGuard: () => () => {} });

/** Pages may register explicit unsaved state; modal forms register automatically. */
export function useWorkspaceCloseGuard(shouldConfirm: boolean) {
  const { registerCloseGuard } = useContext(WorkspaceActivityContext);
  const current = useRef(shouldConfirm);
  current.current = shouldConfirm;
  useEffect(() => registerCloseGuard(() => current.current), [registerCloseGuard]);
}

/** Suspend portalled overlays without changing the page's requested open state. */
export function useWorkspaceOverlay(
  open: boolean | undefined,
  defaultOpen: boolean | undefined,
  onOpenChange: ((open: boolean) => void) | undefined,
) {
  const { active } = useContext(WorkspaceActivityContext);
  const [internalOpen, setInternalOpen] = useState(defaultOpen ?? false);
  const requestedOpen = open ?? internalOpen;
  return {
    active,
    requestedOpen,
    open: active && requestedOpen,
    onOpenChange: (next: boolean) => {
      if (!active) return;
      setInternalOpen(next);
      onOpenChange?.(next);
    },
  };
}
