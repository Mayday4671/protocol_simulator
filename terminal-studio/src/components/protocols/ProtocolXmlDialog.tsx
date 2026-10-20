import { useEffect, useRef, useState } from "react";
import { Loader2, RotateCcw } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { useWorkspaceCloseGuard } from "@/components/layout/WorkspaceActivity";
import { exportProtocolsXmlText, importProtocolsXmlText } from "@/lib/api";
import { XmlEditor, type XmlCheck, type XmlEditorHandle } from "./XmlEditor";

const EMPTY_DOCUMENT = '<?xml version="1.0" encoding="utf-8"?>\n<simterm-import>\n</simterm-import>';

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onApplied: () => void | Promise<void>;
}

/** Keep live XML edits and validation out of the protocol list's render cycle. */
export function ProtocolXmlDialog({ open, onOpenChange, onApplied }: Props) {
  const editorRef = useRef<XmlEditorHandle>(null);
  const applyingRef = useRef(false);
  const [initialText, setInitialText] = useState("");
  const [loading, setLoading] = useState(false);
  const [loadError, setLoadError] = useState("");
  const [retry, setRetry] = useState(0);
  const [applying, setApplying] = useState(false);
  const [applyError, setApplyError] = useState("");
  const [validation, setValidation] = useState<XmlCheck | null>(null);
  const [editorReady, setEditorReady] = useState(false);
  const [dirty, setDirty] = useState(false);

  useWorkspaceCloseGuard(open && (dirty || applying));

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setLoading(true);
    setLoadError("");
    setApplyError("");
    setValidation(null);
    setEditorReady(false);
    setDirty(false);
    // Query the backend even while the list is still loading. Only its explicit
    // empty-library result may produce a new document; other failures remain errors.
    const load = exportProtocolsXmlText([]).catch(error => {
      if (String(error) === "没有可导出的协议") return EMPTY_DOCUMENT;
      throw error;
    });
    void load.then(text => {
      if (!cancelled) setInitialText(text);
    }).catch(error => {
      if (!cancelled) setLoadError(String(error));
    }).finally(() => {
      if (!cancelled) setLoading(false);
    });
    return () => { cancelled = true; };
  }, [open, retry]);

  const changeOpen = (next: boolean) => {
    if (!next && applyingRef.current) return;
    onOpenChange(next);
  };

  const apply = async () => {
    const editor = editorRef.current;
    if (applyingRef.current || loading || !editorReady || !editor) return;
    // Debounced status may be pending. Always validate and read the current
    // model synchronously before sending any data to the Rust import command.
    const checked = editor.validate();
    setValidation(checked);
    if (!checked.ok) { editor.focus(); return; }
    const text = editor.getValue();
    applyingRef.current = true;
    setApplying(true);
    setApplyError("");
    try {
      const result = await importProtocolsXmlText(text);
      const counts = `新增 ${result.imported} / 更新 ${result.updated} 个协议，字典 ${result.dict_sets} 张`;
      const partial = result.failed.length > 0;
      if (partial) {
        const failures = `${result.failed.length} 项失败：\n${result.failed.join("\n")}`;
        setApplyError(`${counts}。${failures}`);
        toast.warning("XML 部分应用，源稿已保留", { description: `${counts}；${result.failed.length} 项失败` });
      }
      try {
        await onApplied();
      } catch (error) {
        const message = `数据已写入，但列表刷新失败：${String(error)}`;
        setApplyError(previous => previous ? `${previous}\n${message}` : message);
        toast.warning("列表刷新失败，源稿已保留", { description: String(error) });
        return;
      }
      if (!partial) {
        toast.success("XML 已应用", { description: counts });
        onOpenChange(false);
      }
    } catch (error) {
      setApplyError(String(error));
      toast.error("XML 应用失败，源稿已保留", { description: String(error) });
    } finally {
      applyingRef.current = false;
      setApplying(false);
    }
  };

  return <Dialog open={open} onOpenChange={changeOpen}>
    <DialogContent aria-describedby={undefined}
      className="flex h-[calc(100vh-6rem)] w-[min(1080px,94vw)] max-w-none flex-col"
      onEscapeKeyDown={event => {
        const target = event.target instanceof Element ? event.target : document.activeElement;
        if (applyingRef.current || target?.closest(".monaco-editor, .monaco-menu, .quick-input-widget, .quickinput")) event.preventDefault();
      }}>
      <DialogHeader className="shrink-0"><DialogTitle>XML 在线编辑</DialogTitle></DialogHeader>
      {loading ? <div className="flex flex-1 items-center justify-center gap-2 text-sm text-muted-foreground" role="status">
        <Loader2 className="h-4 w-4 animate-spin" />正在导出协议…
      </div> : loadError ? <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-3">
        <p className="text-sm text-destructive" role="alert">导出失败</p>
        <p className="max-w-full break-all text-xs text-muted-foreground">{loadError}</p>
        <Button variant="outline" onClick={() => setRetry(value => value + 1)}><RotateCcw />重试</Button>
      </div> : <XmlEditor ref={editorRef} value={initialText} readOnly={applying}
        onDirtyChange={setDirty}
        onValidationChange={checked => { setValidation(checked); if (checked) setEditorReady(true); }} />}
      {applyError && <div role="alert" className="max-h-24 shrink-0 overflow-auto whitespace-pre-wrap break-words rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive">{applyError}</div>}
      <div className="flex shrink-0 items-center justify-end gap-2 border-t pt-3">
        <Button variant="outline" disabled={applying} onClick={() => changeOpen(false)}>取消</Button>
        <Button onClick={() => void apply()} disabled={applying || loading || Boolean(loadError) || !editorReady || validation?.ok === false}>
          {applying ? <><Loader2 className="h-4 w-4 animate-spin" />应用中…</> : "应用(立即生效)"}
        </Button>
      </div>
    </DialogContent>
  </Dialog>;
}
