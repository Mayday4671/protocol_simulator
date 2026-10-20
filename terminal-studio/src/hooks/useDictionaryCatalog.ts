import { useCallback, useContext, useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { WorkspaceActivityContext } from "@/components/layout/WorkspaceActivity";
import { DICTIONARY_CATALOG_CHANGED, listDictSets } from "@/lib/api";
import type { DictSetView } from "@/types";

/** Refresh only the dictionary catalog; protocol fields and unsaved form values are untouched. */
export function useDictionaryCatalog() {
  const { active } = useContext(WorkspaceActivityContext);
  const [dicts, setDicts] = useState<DictSetView[]>([]);
  const mounted = useRef(false);
  const request = useRef(0);

  const refresh = useCallback(async (): Promise<void> => {
    const current = ++request.current;
    try {
      const next = await listDictSets();
      if (mounted.current && current === request.current) setDicts(next);
    } catch (error) {
      if (mounted.current && current === request.current) {
        // Keep a usable catalog if a transient read fails; use one toast across mounted consumers.
        toast.error("字典列表刷新失败", {
          id: "dictionary-catalog-refresh-error",
          description: String(error),
        });
      }
    }
  }, []);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      ++request.current;
    };
  }, []);

  useEffect(() => {
    if (!active) return;
    void refresh();
    const reload = () => { void refresh(); };
    window.addEventListener("focus", reload);
    window.addEventListener(DICTIONARY_CATALOG_CHANGED, reload);
    return () => {
      window.removeEventListener("focus", reload);
      window.removeEventListener(DICTIONARY_CATALOG_CHANGED, reload);
    };
  }, [active, refresh]);

  return { dicts, refresh };
}
