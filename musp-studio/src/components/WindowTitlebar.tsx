import {useEffect,useState,type MouseEvent,type ReactNode} from 'react';
import {isTauri} from '@tauri-apps/api/core';
import {getCurrentWindow} from '@tauri-apps/api/window';

export function WindowTitlebar({children,onError}:{children:ReactNode;onError?:(message:string)=>void}){
 const native=isTauri();const [maximized,setMaximized]=useState(false);
 useEffect(()=>{
  if(!native)return;
  let disposed=false;let unlisten:(()=>void)|undefined;const appWindow=getCurrentWindow();
  const update=()=>{void appWindow.isMaximized().then(value=>{if(!disposed)setMaximized(value)}).catch(error=>onError?.('读取窗口状态失败：'+String(error)))};
  update();void appWindow.onResized(update).then(stop=>{if(disposed)stop();else unlisten=stop}).catch(error=>onError?.('监听窗口状态失败：'+String(error)));
  return()=>{disposed=true;unlisten?.()};
 },[native]);
 async function act(action:'minimize'|'toggleMaximize'|'close'|'startDragging'){
  if(!native)return;
  try{const appWindow=getCurrentWindow();await appWindow[action]();if(action==='toggleMaximize')setMaximized(await appWindow.isMaximized())}
  catch(error){onError?.('窗口操作失败：'+String(error))}
 }
 function dragTarget(event:MouseEvent<HTMLElement>){const target=event.target;if(!(target instanceof Element))return false;if(target.closest('button,input,select,textarea,a,[role="button"],[role="combobox"],.ant-select,.window-controls'))return false;return target===event.currentTarget||!!target.closest('[data-window-drag]')}
 return <header className="titlebar unified-titlebar" onMouseDown={event=>{if(event.button===0&&event.detail<2&&dragTarget(event)){event.preventDefault();void act('startDragging')}}} onDoubleClick={event=>{if(dragTarget(event)){event.preventDefault();void act('toggleMaximize')}}}>
  {children}
  <div className="window-controls" aria-label="桌面窗口控制">
   <button className="window-control" aria-label="最小化窗口" title="最小化" disabled={!native} onClick={()=>void act('minimize')}><svg aria-hidden="true" width="12" height="12" viewBox="0 0 12 12"><path d="M1 6.5H11"/></svg></button>
   <button className="window-control" aria-label={maximized?'还原窗口':'最大化窗口'} title={maximized?'还原':'最大化'} disabled={!native} onClick={()=>void act('toggleMaximize')}><svg aria-hidden="true" width="12" height="12" viewBox="0 0 12 12">{maximized?<><path d="M3.5 3.5V1.5H10.5V8.5H8.5"/><rect x="1.5" y="3.5" width="7" height="7"/></>:<rect x="1.5" y="1.5" width="9" height="9"/>}</svg></button>
   <button className="window-control window-close" aria-label="关闭窗口" title="关闭" disabled={!native} onClick={()=>void act('close')}><svg aria-hidden="true" width="12" height="12" viewBox="0 0 12 12"><path d="M1.5 1.5L10.5 10.5M10.5 1.5L1.5 10.5"/></svg></button>
  </div>
 </header>;
}
