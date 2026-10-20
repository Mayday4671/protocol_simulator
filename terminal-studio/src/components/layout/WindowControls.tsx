import { useEffect, useState } from 'react';
import { createPortal } from 'react-dom';
import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Minus, Square, Copy, X } from 'lucide-react';
import { toast } from 'sonner';

export function WindowControls() {
  const [maximized, setMaximized] = useState(false);
  const native = isTauri();
  useEffect(() => {
    if (!native) return;
    const appWindow = getCurrentWindow();
    let disposed = false;
    let unlisten: (() => void) | undefined;
    const update = async () => {
      const value = await appWindow.isMaximized();
      if (!disposed) setMaximized(value);
    };
    void update().catch(() => {});
    void appWindow.onResized(() => { void update().catch(() => {}); }).then(off => {
      if (disposed) off(); else unlisten = off;
    }).catch(() => {});
    return () => { disposed = true; unlisten?.(); };
  }, [native]);
  const action = (name: 'minimize' | 'toggleMaximize' | 'close') => {
    if (native) void getCurrentWindow()[name]().catch(error => toast.error(String(error)));
  };
  return createPortal(<div className="window-controls" aria-label="窗口控制">
    <button type="button" aria-label="最小化窗口" title="最小化" onClick={() => action('minimize')}><Minus size={16} /></button>
    <button type="button" aria-label={maximized ? '还原窗口' : '最大化窗口'} title={maximized ? '还原' : '最大化'} onClick={() => action('toggleMaximize')}>{maximized ? <Copy size={13} /> : <Square size={13} />}</button>
    <button type="button" className="window-close" aria-label="关闭窗口" title="关闭" onClick={() => action('close')}><X size={18} /></button>
  </div>, document.body);
}
