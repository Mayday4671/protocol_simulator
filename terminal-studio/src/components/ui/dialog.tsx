import * as React from "react";
import { createPortal } from "react-dom";
import * as DialogPrimitive from "@radix-ui/react-dialog";
import { X } from "lucide-react";

import { cn } from "@/lib/utils";
import { useWorkspaceCloseGuard, useWorkspaceOverlay, WorkspaceActivityContext } from "@/components/layout/WorkspaceActivity";

const DialogEditingContext = React.createContext<(editable: boolean) => void>(() => {});
const DialogDisplayContext = React.createContext({ requestedOpen: false, inWorkspace: false });

function Dialog({ open, defaultOpen, onOpenChange, children, ...props }: React.ComponentProps<typeof DialogPrimitive.Root>) {
  const overlay = useWorkspaceOverlay(open, defaultOpen, onOpenChange);
  const { inWorkspace = false } = React.useContext(WorkspaceActivityContext);
  const [editable, setEditable] = React.useState(false);
  // Closing an editing tab is destructive even if its modal is temporarily suspended.
  useWorkspaceCloseGuard(overlay.requestedOpen && editable);
  return <DialogEditingContext.Provider value={setEditable}>
    <DialogDisplayContext.Provider value={{ requestedOpen: overlay.requestedOpen, inWorkspace }}>
      <DialogPrimitive.Root {...props} modal={inWorkspace ? false : props.modal} open={overlay.open} onOpenChange={overlay.onOpenChange}>{children}</DialogPrimitive.Root>
    </DialogDisplayContext.Provider>
  </DialogEditingContext.Provider>;
}

const DialogTrigger = DialogPrimitive.Trigger;

const DialogPortal = DialogPrimitive.Portal;

const DialogClose = DialogPrimitive.Close;

const DialogOverlay = React.forwardRef<
  React.ElementRef<typeof DialogPrimitive.Overlay>,
  React.ComponentPropsWithoutRef<typeof DialogPrimitive.Overlay>
>(({ className, ...props }, ref) => (
  <DialogPrimitive.Overlay
    ref={ref}
    className={cn(
      "fixed inset-0 z-50 bg-black/80 data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0",
      className,
    )}
    {...props}
  />
));
DialogOverlay.displayName = DialogPrimitive.Overlay.displayName;

const DialogContent = React.forwardRef<
  React.ElementRef<typeof DialogPrimitive.Content>,
  React.ComponentPropsWithoutRef<typeof DialogPrimitive.Content>
>(({ className, children, onCloseAutoFocus, onOpenAutoFocus, onInteractOutside, style, ...props }, ref) => {
  const { active } = React.useContext(WorkspaceActivityContext);
  const { requestedOpen, inWorkspace } = React.useContext(DialogDisplayContext);
  const activeRef = React.useRef(active); activeRef.current = active;
  const scrolls = React.useRef(new Map<HTMLElement, [number, number]>());
  // React keeps rendering into one stable container even while Radix's focus/interaction
  // shell is suspended. Reattaching the container retains child drafts and scroll too.
  const [container] = React.useState(() => { const el = document.createElement('div'); el.style.display = 'contents'; return el; });
  const [bounds, setBounds] = React.useState<React.CSSProperties>({});
  React.useLayoutEffect(() => {
    if (!inWorkspace || !active || !requestedOpen) return;
    const panel = document.querySelector<HTMLElement>('[role="tabpanel"]:not([hidden])');
    if (!panel) return;
    const measure = () => {
      const r = panel.getBoundingClientRect();
      setBounds({ left: r.left+r.width/2, top: r.top+r.height/2, width: Math.max(0,r.width-32), maxHeight: Math.max(0,r.height-32) });
    };
    measure();
    const observer = new ResizeObserver(measure); observer.observe(panel);
    return () => observer.disconnect();
  }, [active, inWorkspace, requestedOpen]);
  const setEditable = React.useContext(DialogEditingContext);
  const contentRef = React.useRef<HTMLDivElement>(null);
  React.useImperativeHandle(ref, () => contentRef.current!);
  React.useLayoutEffect(() => {
    if (contentRef.current) setEditable(Boolean(contentRef.current.querySelector('input:not([readonly]):not([disabled]):not([type="hidden"]), textarea:not([readonly]):not([disabled]), select:not([disabled]), [contenteditable="true"], [role="combobox"], [role="switch"]')));
  });
  React.useEffect(() => {
    if (!requestedOpen) { scrolls.current.clear(); return; }
    const inspect = () => setEditable(Boolean(container.querySelector('input:not([readonly]):not([disabled]):not([type="hidden"]), textarea:not([readonly]):not([disabled]), select:not([disabled]), [contenteditable="true"], [role="combobox"], [role="switch"]')));
    inspect();
    const observer = new MutationObserver(inspect); observer.observe(container, { childList: true, subtree: true });
    return () => observer.disconnect();
  }, [container, requestedOpen, setEditable]);
  const attach = React.useCallback((host: HTMLDivElement | null) => {
    if (!host) {
      scrolls.current.clear();
      for (const el of container.querySelectorAll<HTMLElement>('*')) {
        if (el.scrollTop || el.scrollLeft) scrolls.current.set(el, [el.scrollTop, el.scrollLeft]);
      }
    }
    contentRef.current = host;
    if (host) {
      host.appendChild(container);
      requestAnimationFrame(() => {
        if (contentRef.current !== host) return;
        for (const [el, [top, left]] of scrolls.current) { el.scrollTop = top; el.scrollLeft = left; }
      });
    }
  }, [container]);
  return (
  <>
  {requestedOpen && createPortal(<>{children}
      <DialogPrimitive.Close className="absolute right-4 top-4 rounded-sm opacity-70 ring-offset-background transition-opacity hover:opacity-100 focus:outline-none disabled:pointer-events-none">
        <X className="h-4 w-4" /><span className="sr-only">关闭</span>
      </DialogPrimitive.Close>
    </>, container)}
  {active && <DialogPortal>
    <DialogOverlay />
    <DialogPrimitive.Content
      ref={attach}
      className={cn(
        "fixed left-[50%] top-[50%] z-50 flex w-full max-w-lg flex-col translate-x-[-50%] translate-y-[-50%] gap-4 overflow-hidden border bg-background p-6 shadow-lg duration-200 data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-95 data-[state=open]:fade-in-0 data-[state=open]:zoom-in-95 max-h-[calc(100vh-3rem)] sm:rounded-lg",
        className,
      )}
      {...props}
      style={{ ...bounds, ...style }}
      onOpenAutoFocus={event => {
        if (scrolls.current.size) event.preventDefault();
        onOpenAutoFocus?.(event);
      }}
      onInteractOutside={event => {
        // Workspace editors do not dismiss when navigating to another tab/page.
        if (inWorkspace) event.preventDefault();
        onInteractOutside?.(event);
      }}
      onCloseAutoFocus={event => {
        // A suspended page must not move focus back into its hidden trigger.
        if (!activeRef.current) event.preventDefault();
        onCloseAutoFocus?.(event);
      }}
    >
    </DialogPrimitive.Content>
  </DialogPortal>}
  </>
  );
});
DialogContent.displayName = DialogPrimitive.Content.displayName;

const DialogHeader = ({
  className,
  ...props
}: React.HTMLAttributes<HTMLDivElement>) => (
  <div
    className={cn(
      "flex flex-col space-y-1.5 text-center sm:text-left",
      className,
    )}
    {...props}
  />
);
DialogHeader.displayName = "DialogHeader";

const DialogFooter = ({
  className,
  ...props
}: React.HTMLAttributes<HTMLDivElement>) => (
  <div
    className={cn(
      "flex shrink-0 flex-col-reverse -mx-6 -mb-6 border-t bg-background px-6 py-4 sm:flex-row sm:justify-end sm:space-x-2",
      className,
    )}
    {...props}
  />
);
DialogFooter.displayName = "DialogFooter";

const DialogTitle = React.forwardRef<
  React.ElementRef<typeof DialogPrimitive.Title>,
  React.ComponentPropsWithoutRef<typeof DialogPrimitive.Title>
>(({ className, ...props }, ref) => (
  <DialogPrimitive.Title
    ref={ref}
    className={cn(
      "text-lg font-semibold leading-none tracking-tight",
      className,
    )}
    {...props}
  />
));
DialogTitle.displayName = DialogPrimitive.Title.displayName;

const DialogDescription = React.forwardRef<
  React.ElementRef<typeof DialogPrimitive.Description>,
  React.ComponentPropsWithoutRef<typeof DialogPrimitive.Description>
>(({ className, ...props }, ref) => (
  <DialogPrimitive.Description
    ref={ref}
    className={cn("text-sm text-muted-foreground", className)}
    {...props}
  />
));
DialogDescription.displayName = DialogPrimitive.Description.displayName;

export {
  Dialog,
  DialogPortal,
  DialogOverlay,
  DialogTrigger,
  DialogClose,
  DialogContent,
  DialogHeader,
  DialogFooter,
  DialogTitle,
  DialogDescription,
};
