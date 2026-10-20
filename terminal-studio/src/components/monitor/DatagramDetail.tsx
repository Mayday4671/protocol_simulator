import { useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";
import { ArrowLeft, ChevronDown, ChevronsDownUp, ChevronsUpDown, ClipboardCopy, Pin, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { parseDatagram } from "@/lib/api";
import { cn } from "@/lib/utils";
import type { DatagramParse } from "@/types";
import { buildDatagramSections, datagramEndpoints, formatByteRange, type DatagramDisplayField } from "./datagramSections";
import "./DatagramDetail.css";

function displayHex(hex: string) {
  return hex.replace(/^0x/i, "").match(/.{1,2}/g)?.join(" ") || "—";
}

// Keep a named section's shade stable when the same header appears in other messages.
function sectionTone(name: string) {
  let hash = 0;
  for (const char of name) hash = (hash * 31 + char.charCodeAt(0)) >>> 0;
  return hash % 3;
}

function fieldColorKey(field: DatagramDisplayField) {
  return [field.sectionKind, field.sectionName, field.record_name ?? "", field.displayName].join("/");
}

// Values, byte offsets and record indices must not change a field's visual identity.
function fieldToneSeed(field: DatagramDisplayField) {
  let hash = 2166136261;
  for (const char of fieldColorKey(field)) hash = Math.imul(hash ^ char.charCodeAt(0), 16777619);
  return (hash >>> 0) % 6;
}

export function DatagramDetail({ logId, onClose }: { logId: number; onClose: () => void }) {
  const [data, setData] = useState<DatagramParse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [hover, setHover] = useState<string | null>(null);
  const [pinned, setPinned] = useState<string | null>(null);
  const [collapsed, setCollapsed] = useState<Set<string>>(() => new Set());
  const overlayRef = useRef<HTMLDivElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    let cancelled = false;
    setData(null);
    setError(null);
    setHover(null);
    setPinned(null);
    setCollapsed(new Set());
    parseDatagram(logId).then(
      result => { if (!cancelled) setData(result); },
      reason => { if (!cancelled) setError(String(reason)); },
    );
    return () => { cancelled = true; };
  }, [logId]);

  useEffect(() => {
    const previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    closeRef.current?.focus();
    return () => { if (previousFocus?.isConnected) previousFocus.focus(); };
  }, []);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        if (pinned !== null) {
          setPinned(null);
          setHover(null);
        } else onClose();
      }
      if (event.key === "Tab") {
        const buttons = Array.from(overlayRef.current?.querySelectorAll<HTMLElement>("button:not([disabled]), [tabindex='0']") ?? [])
          .filter(button => button.getClientRects().length > 0);
        if (!buttons?.length) return;
        const first = buttons[0];
        const last = buttons[buttons.length - 1];
        if (event.shiftKey && document.activeElement === first) {
          event.preventDefault();
          last.focus();
        } else if (!event.shiftKey && document.activeElement === last) {
          event.preventDefault();
          first.focus();
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose, pinned]);

  const sections = useMemo(() => data ? buildDatagramSections(data.fields, data.raw_hex) : [], [data]);
  const fieldTones = useMemo(() => {
    const assigned = new Map<string, number>();
    const tones = new Map<string, number>();
    for (const section of sections) {
      for (const group of section.groups) {
        const used = new Set<number>();
        for (const field of group.fields) {
          const key = fieldColorKey(field);
          let tone = assigned.get(key);
          if (tone === undefined) {
            tone = fieldToneSeed(field);
            while (used.has(tone)) tone = (tone + 1) % 6;
            assigned.set(key, tone);
          }
          tones.set(field.id, tone);
          used.add(tone);
          if (used.size === 6) {
            used.clear();
            used.add(tone);
          }
        }
      }
    }
    return tones;
  }, [sections]);
  const selected = sections.flatMap(section => section.fields).find(field => field.id === pinned);
  const endpoints = data ? datagramEndpoints(data) : null;
  const rawHex = data ? displayHex(data.raw_hex) : "";
  const active = pinned ?? hover;

  const toggleSection = (id: string) => {
    if (!collapsed.has(id)) {
      const section = sections.find(item => item.id === id);
      if (section?.fields.some(field => field.id === pinned)) setPinned(null);
      setHover(null);
    }
    setCollapsed(previous => {
      const next = new Set(previous);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const revealSection = (id: string) => {
    setCollapsed(previous => {
      const next = new Set(previous);
      next.delete(id);
      return next;
    });
    requestAnimationFrame(() => {
      const section = overlayRef.current?.querySelector<HTMLElement>(`#${id}`);
      section?.querySelector<HTMLButtonElement>(".datagram-section-heading")?.focus({ preventScroll: true });
      section?.scrollIntoView({ behavior: "auto", block: "start" });
    });
  };

  const copyText = useMemo(() => {
    if (!data) return "";
    const route = datagramEndpoints(data);
    return [
      `报文解析 — ${data.protocol_name}`,
      `${data.direction === "in" ? "接收" : "发送"} ${data.ts} ${route.source} → ${route.target}`,
      `总长: ${data.total_len} 字节`,
      ...sections.flatMap(section => [
        `\n${section.name} [${formatByteRange(section.start, section.byteLength)}] ${section.byteLength} B`,
        ...section.fields.map(field =>
          `[${formatByteRange(field.start, field.actualLength)}] ${field.name}: ${field.hex || "—"}` +
          (field.dec !== null ? ` · DEC ${field.dec}` : "") +
          (field.dict_label ? ` · ${field.dict_label}` : "") +
          (field.meaning ? ` · ${field.meaning}` : "")),
      ]),
    ].join("\n");
  }, [data, sections]);

  const copy = async (text: string, success: string) => {
    try {
      await navigator.clipboard.writeText(text);
      toast.success(success);
    } catch { toast.error("复制失败"); }
  };

  const renderField = (field: DatagramDisplayField) => (
    <button
      key={field.id}
      type="button"
      className={cn("datagram-field", active === field.id && "datagram-field-active")}
      data-field-tone={field.sectionKind === "unparsed" ? undefined : fieldTones.get(field.id)}
      data-field-key={fieldColorKey(field)}
      aria-label={`${field.displayName}，字节 ${formatByteRange(field.start, field.actualLength)}，${field.actualLength} 字节`}
      aria-pressed={pinned === field.id}
      onMouseEnter={() => setHover(field.id)}
      onMouseLeave={() => setHover(null)}
      onFocus={() => setHover(field.id)}
      onBlur={() => setHover(null)}
      onClick={() => setPinned(value => value === field.id ? null : field.id)}
    >
      <span className="datagram-field-top">
        <span className="datagram-offset">{formatByteRange(field.start, field.actualLength).replace(/^@/, "")}</span>
        <span className="datagram-field-meta"><span className="datagram-length">{field.actualLength} B</span>{pinned === field.id && <Pin size={12} className="datagram-field-pin" aria-hidden="true" />}</span>
      </span>
      <span className="datagram-field-name">{field.displayName}</span>
      <span className="datagram-field-hex"><span className="datagram-value-label">HEX</span>{displayHex(field.actualLength > 24 ? field.hex.slice(0, 50) : field.hex)}{field.actualLength > 24 && " …"}</span>
      <span className={cn("datagram-field-dec", (field.dec?.length ?? 0) > 14 && "datagram-field-dec-long")}><span className="datagram-value-label">DEC</span>{field.dec ?? "—"}</span>
      {field.dict_label && <span className="datagram-dictionary">{field.dict_label}</span>}
      {field.actualLength > 24 && <span className="datagram-expand">查看全部 {field.actualLength} B</span>}
      {field.actualLength < (field.expected_len ?? field.len) && <span className="datagram-incomplete">需 {field.expected_len ?? field.len} B · 缺 {(field.expected_len ?? field.len) - field.actualLength} B</span>}
      {field.actualLength === 0 && <span className="datagram-empty-field">{field.meaning || "无数据"}</span>}
    </button>
  );

  return (
    <div ref={overlayRef} className="app-overlay datagram-detail" role="dialog" aria-modal="true" aria-labelledby="datagram-title">
      <header className="datagram-toolbar">
        <Button ref={closeRef} variant="ghost" size="icon" onClick={onClose} aria-label="返回" title="返回 (Esc)"><ArrowLeft /></Button>
        <h2 id="datagram-title">报文解析</h2>
        {data && <span className="datagram-summary"><strong>{data.total_len}</strong> 字节<span>·</span>{data.fields.length} 个字段</span>}
        <div className="datagram-actions">
          <Button variant="outline" size="sm" disabled={!sections.length} aria-label="全部折叠" title="全部折叠" onClick={() => {
            setCollapsed(new Set(sections.map(section => section.id)));
            setPinned(null);
            setHover(null);
          }}><ChevronsDownUp /><span>全部折叠</span></Button>
          <Button variant="outline" size="sm" disabled={!sections.length} aria-label="全部展开" title="全部展开" onClick={() => setCollapsed(new Set())}><ChevronsUpDown /><span>全部展开</span></Button>
          <Button variant="outline" size="sm" onClick={() => copy(copyText, "解析文本已复制到剪贴板")} disabled={!data} aria-label="复制解析"><ClipboardCopy /><span>复制解析</span></Button>
          <Button variant="outline" size="sm" onClick={() => copy(rawHex, "报文十六进制已复制到剪贴板")} disabled={!data?.raw_hex} aria-label="复制报文"><ClipboardCopy /><span>复制报文</span></Button>
          <Button variant="ghost" size="icon" onClick={onClose} aria-label="关闭"><X /></Button>
        </div>
      </header>

      <main className="datagram-scroll">
        {error && <div className="datagram-error" role="alert">{error}</div>}
        {!data && !error && <div className="datagram-loading" role="status">解析中…</div>}
        {data && endpoints && <>
          <div className="datagram-metadata">
            <div className="datagram-protocol-line">
              <h3>{data.protocol_name || "未识别报文"}</h3>
              <span className="datagram-direction">{data.direction === "in" ? "接收" : "发送"}</span>
              <time>{data.ts}</time>
            </div>
            <div className="datagram-route"><span>{endpoints.source}</span><span aria-hidden="true">→</span><span>{endpoints.target}</span>{data.node_name && <span className="datagram-node">{data.node_name}</span>}</div>
          </div>

          {sections.length > 1 && <nav className="datagram-navigation" aria-label="报文分区">
            <div className="datagram-segment-bar" aria-hidden="true">{sections.map(section => <span key={section.id} data-kind={section.kind} data-tone={sectionTone(section.name)} style={{ flexGrow: section.byteLength || 0.3 }} />)}</div>
            <div className="datagram-section-links">{sections.map(section => <button
              key={section.id} type="button" data-kind={section.kind} data-tone={sectionTone(section.name)}
              onClick={() => revealSection(section.id)}
            ><span className="datagram-section-dot" />{section.name}<span>{section.byteLength} B</span></button>)}</div>
          </nav>}

          <div className="datagram-sections">
            {sections.map(section => <section key={section.id} id={section.id} data-kind={section.kind} data-tone={sectionTone(section.name)} data-collapsed={collapsed.has(section.id)} className="datagram-section" aria-labelledby={`${section.id}-title`}>
              <h3 className="datagram-section-heading-wrap"><button type="button" className="datagram-section-heading" aria-expanded={!collapsed.has(section.id)} aria-controls={`${section.id}-content`} onClick={() => toggleSection(section.id)} title={collapsed.has(section.id) ? "展开分区" : "折叠分区"}>
                <ChevronDown className="datagram-section-chevron" size={16} aria-hidden="true" />
                <span id={`${section.id}-title`} className="datagram-section-title">{section.name}</span>
                <span className="datagram-section-range">字节 <strong>{formatByteRange(section.start, section.byteLength)}</strong></span>
                <span className="datagram-section-count">{section.fields.length} 个字段</span>
                <span className="datagram-section-size">{section.byteLength}<span>B</span></span>
              </button></h3>
              <div id={`${section.id}-content`} className="datagram-section-content" hidden={collapsed.has(section.id)}>{section.groups.map(group => {
                const isRecord = group.recordName !== null || group.recordIndex !== null;
                return <div key={group.id} className={cn("datagram-field-group", isRecord && "datagram-record")}>
                  {isRecord && <div className="datagram-record-heading"><h4 className="datagram-record-title">{group.recordName || "记录"}{group.recordIndex !== null && <span className="datagram-record-number">第 {group.recordIndex} 条</span>}</h4><span>{group.fields.reduce((sum, field) => sum + field.actualLength, 0)} B</span></div>}
                  <div className="datagram-field-grid">{group.fields.map(renderField)}</div>
                </div>;
              })}</div>
            </section>)}
            {!sections.length && <div className="datagram-loading">无有效字节</div>}
          </div>
        </>}
      </main>

      {selected && <aside className="datagram-inspector" aria-label="选中字段详情">
        <div className="datagram-inspector-heading"><Pin size={14} /><strong>{selected.name}</strong><span>{selected.sectionName} · 字节 {formatByteRange(selected.start, selected.actualLength)} · {selected.actualLength} B</span><Button variant="ghost" size="icon" onClick={() => { setPinned(null); setHover(null); }} aria-label="取消锁定"><X /></Button></div>
        <div className="datagram-inspector-values">
          <div><span className="datagram-value-label">HEX</span><code>{displayHex(selected.hex)}</code></div>
          <div><span className="datagram-value-label">DEC</span><code>{selected.dec ?? "—"}</code></div>
          {(selected.dict_label || selected.meaning) && <div><span className="datagram-value-label">含义</span><span>{[selected.dict_label, selected.meaning].filter(Boolean).filter((item, index, list) => list.indexOf(item) === index).join(" · ")}</span></div>}
        </div>
      </aside>}
    </div>
  );
}
