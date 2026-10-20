import { forwardRef, useContext, useEffect, useImperativeHandle, useRef, useState } from "react";
import type { editor as MonacoEditor } from "monaco-editor/editor/editor.api";
import { Check, ChevronsDownUp, ChevronsUpDown, Loader2, Replace, Search, TriangleAlert, WrapText } from "lucide-react";
import { Button } from "@/components/ui/button";
import { WorkspaceActivityContext } from "@/components/layout/WorkspaceActivity";
import { cn } from "@/lib/utils";
import { checkXml, type XmlCheck } from "./xmlValidation";

export { checkXml, type XmlCheck } from "./xmlValidation";
export interface XmlEditorHandle {
  getValue: () => string;
  validate: () => XmlCheck;
  focus: () => void;
}
interface Props {
  value: string;
  onChange?: (value: string) => void;
  onValidationChange?: (check: XmlCheck | null) => void;
  onDirtyChange?: (dirty: boolean) => void;
  readOnly?: boolean;
  className?: string;
}

/** Monaco owns the live document. The application only reads it on apply. */
export const XmlEditor = forwardRef<XmlEditorHandle, Props>(function XmlEditor(props, ref) {
  const host = useRef<HTMLDivElement>(null);
  const instance = useRef<MonacoEditor.IStandaloneCodeEditor | null>(null);
  const callbacks = useRef(props); callbacks.current = props;
  const validateRef = useRef<() => XmlCheck>(() => checkXml(callbacks.current.value));
  const externalValue = useRef(props.value);
  const { active } = useContext(WorkspaceActivityContext);
  const activeRef = useRef(active); activeRef.current = active;
  const [ready, setReady] = useState(false);
  const [loadError, setLoadError] = useState("");
  const [check, setCheck] = useState<XmlCheck | null>(null);
  const [position, setPosition] = useState({ lineNumber: 1, column: 1 });
  const [lines, setLines] = useState(1);
  const [wrap, setWrap] = useState(false);

  useImperativeHandle(ref, () => ({
    getValue: () => instance.current?.getValue() ?? callbacks.current.value,
    validate: () => validateRef.current(),
    focus: () => instance.current?.focus(),
  }), []);

  useEffect(() => {
    let disposed = false;
    let disposeEditor: (() => void) | undefined;
    void import("./monacoXml").then(({ monaco }) => {
      if (disposed || !host.current) return;
      const model = monaco.editor.createModel(callbacks.current.value, "xml");
      model.updateOptions({ tabSize: 2, insertSpaces: true });
      let originalVersion = model.getAlternativeVersionId();
      let composition = false;
      let timer: ReturnType<typeof setTimeout> | undefined;
      const editor = monaco.editor.create(host.current, {
        model, ariaLabel: "XML 源码", automaticLayout: true,
        fontFamily: 'Consolas, "Courier New", "Microsoft YaHei", monospace',
        fontSize: 13, lineHeight: 22, fontLigatures: false,
        minimap: { enabled: false }, scrollBeyondLastLine: false,
        lineNumbersMinChars: 4, glyphMargin: false, folding: true,
        showFoldingControls: "mouseover", renderLineHighlight: "all",
        overviewRulerLanes: 0, hideCursorInOverviewRuler: true,
        padding: { top: 10, bottom: 10 }, wordWrap: "off",
        smoothScrolling: false, cursorSmoothCaretAnimation: "off",
        stickyScroll: { enabled: false }, roundedSelection: false,
        unicodeHighlight: { ambiguousCharacters: false, nonBasicASCII: false },
        quickSuggestions: false, wordBasedSuggestions: "off",
        readOnly: callbacks.current.readOnly ?? false, editContext: false,
        scrollbar: { verticalScrollbarSize: 10, horizontalScrollbarSize: 10, alwaysConsumeMouseWheel: false },
      });
      instance.current = editor;
      const applyTheme = () => monaco.editor.setTheme(document.documentElement.classList.contains("dark") ? "simterm-xml-dark" : "simterm-xml-light");
      applyTheme();
      const themeObserver = new MutationObserver(applyTheme);
      themeObserver.observe(document.documentElement, { attributes: true, attributeFilter: ["class"] });
      const validate = () => {
        clearTimeout(timer);
        const result = checkXml(model.getValue());
        const line = Math.max(1, Math.min(result.line ?? 1, model.getLineCount()));
        const column = Math.max(1, Math.min(result.column ?? 1, model.getLineMaxColumn(line)));
        monaco.editor.setModelMarkers(model, "xml-syntax", result.ok ? [] : [{
          severity: monaco.MarkerSeverity.Error, message: result.msg,
          startLineNumber: line, startColumn: column,
          endLineNumber: line, endColumn: Math.min(column + 1, model.getLineMaxColumn(line)),
        }]);
        setCheck(result);
        callbacks.current.onValidationChange?.(result);
        return result;
      };
      validateRef.current = validate;
      const scheduleValidation = () => {
        clearTimeout(timer);
        setCheck(null);
        callbacks.current.onValidationChange?.(null);
        monaco.editor.setModelMarkers(model, "xml-syntax", []);
        if (!composition) timer = setTimeout(validate, 300);
      };
      const subscriptions = [
        editor.onDidChangeModelContent(event => {
          if (event.isFlush) originalVersion = model.getAlternativeVersionId();
          callbacks.current.onChange?.(model.getValue());
          callbacks.current.onDirtyChange?.(model.getAlternativeVersionId() !== originalVersion);
          setLines(model.getLineCount());
          scheduleValidation();
        }),
        editor.onDidChangeCursorPosition(event => setPosition(event.position)),
        editor.onDidCompositionStart(() => { composition = true; clearTimeout(timer); }),
        editor.onDidCompositionEnd(() => { composition = false; scheduleValidation(); }),
      ];
      setLines(model.getLineCount());
      setReady(true);
      validate();
      if (activeRef.current && host.current.isConnected) editor.focus();
      disposeEditor = () => {
        clearTimeout(timer);
        themeObserver.disconnect();
        subscriptions.forEach(subscription => subscription.dispose());
        instance.current = null;
        editor.dispose();
        model.dispose();
      };
    }).catch(error => {
      if (!disposed) setLoadError(`编辑器加载失败：${String(error)}`);
    });
    return () => { disposed = true; disposeEditor?.(); };
  }, []);

  // Only external document replacement calls setValue; typing preserves undo history.
  useEffect(() => {
    if (props.value !== externalValue.current) {
      externalValue.current = props.value;
      if (instance.current && instance.current.getValue() !== props.value) instance.current.setValue(props.value);
    }
  }, [props.value]);
  useEffect(() => { instance.current?.updateOptions({ readOnly: props.readOnly ?? false }); }, [props.readOnly, ready]);
  useEffect(() => {
    if (!active) return;
    const frame = requestAnimationFrame(() => instance.current?.layout());
    return () => cancelAnimationFrame(frame);
  }, [active]);

  const run = (id: string) => { instance.current?.focus(); void instance.current?.getAction(id)?.run(); };
  const locateError = () => {
    if (!check || check.ok) return;
    const model = instance.current?.getModel();
    if (!model) return;
    const lineNumber = Math.max(1, Math.min(check.line ?? 1, model.getLineCount()));
    const at = { lineNumber, column: Math.max(1, Math.min(check.column ?? 1, model.getLineMaxColumn(lineNumber))) };
    instance.current?.setPosition(at);
    instance.current?.revealPositionInCenter(at);
    instance.current?.focus();
  };

  return <div className={cn("xml-editor flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-md border bg-background", props.className)}>
    <div className="flex shrink-0 flex-wrap items-center gap-1 border-b bg-muted/25 px-2 py-1" role="toolbar" aria-label="XML 编辑工具">
      <Button variant="ghost" size="sm" disabled={!ready} title="查找 (Ctrl+F)" onClick={() => run("actions.find")}><Search />查找</Button>
      <Button variant="ghost" size="sm" disabled={!ready || props.readOnly} title="替换 (Ctrl+H)" onClick={() => run("editor.action.startFindReplaceAction")}><Replace />替换</Button>
      <span className="mx-1 h-4 border-l" />
      <Button variant="ghost" size="sm" disabled={!ready} onClick={() => run("editor.foldAll")}><ChevronsDownUp />全部折叠</Button>
      <Button variant="ghost" size="sm" disabled={!ready} onClick={() => run("editor.unfoldAll")}><ChevronsUpDown />全部展开</Button>
      <Button variant="ghost" size="sm" disabled={!ready} aria-pressed={wrap} className={cn("ml-auto", wrap && "bg-accent")} onClick={() => {
        instance.current?.updateOptions({ wordWrap: wrap ? "off" : "on" }); setWrap(!wrap);
      }}><WrapText />自动换行</Button>
    </div>
    <div className="relative min-h-0 flex-1">
      <div ref={host} className="absolute inset-0" />
      {!ready && <div className="absolute inset-0 flex items-center justify-center gap-2 bg-background text-sm text-muted-foreground" role="status">
        {loadError || <><Loader2 className="h-4 w-4 animate-spin" />正在加载编辑器…</>}
      </div>}
    </div>
    <div className="flex min-h-8 shrink-0 items-center gap-3 border-t bg-muted/25 px-3 py-1 text-xs">
      <div role="status" aria-live="polite" className={cn("flex min-w-0 flex-1 items-center gap-1.5", check ? check.ok ? "text-emerald-700 dark:text-emerald-400" : "text-destructive" : "text-muted-foreground")}>
        {check ? check.ok ? <Check className="h-3.5 w-3.5 shrink-0" /> : <TriangleAlert className="h-3.5 w-3.5 shrink-0" /> : <Loader2 className="h-3.5 w-3.5 shrink-0 animate-spin" />}
        <span className="truncate" title={check?.msg}>{check?.msg ?? (ready ? "等待校验…" : "加载中…")}</span>
        {check && !check.ok && <button type="button" className="shrink-0 underline underline-offset-2" onClick={locateError}>定位错误</button>}
      </div>
      <span className="shrink-0 tabular-nums text-muted-foreground" aria-label="光标位置">行 {position.lineNumber}，列 {position.column}<span className="ml-3 hidden sm:inline">共 {lines} 行</span></span>
    </div>
  </div>;
});
