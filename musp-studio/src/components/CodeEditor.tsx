import Editor,{loader,type OnMount} from '@monaco-editor/react';
import * as monaco from 'monaco-editor';
import EditorWorker from 'monaco-editor/editor/editor.worker?worker';
import JsonWorker from 'monaco-editor/language/json/json.worker?worker';
import {useUI} from '../store/ui';
import type {Diagnostic} from '../api/tauri';
import {useEffect,useRef} from 'react';
// Bundle Monaco and its worker locally: no CDN or internet dependency.
self.MonacoEnvironment={getWorker:(_workerId,label)=>label==='json'?new JsonWorker():new EditorWorker()};loader.config({monaco});
export function CodeEditor({value,onChange,language='xml',height='100%',diagnostics=[],onSave,readOnly=false}:{value:string;onChange?:(v:string)=>void;language?:string;height?:string|number;diagnostics?:Diagnostic[];onSave?:()=>void;readOnly?:boolean}){const dark=useUI(s=>s.dark);const ref=useRef<monaco.editor.IStandaloneCodeEditor|null>(null);const saveRef=useRef(onSave);saveRef.current=onSave;const onMount:OnMount=(editor)=>{ref.current=editor;if(onSave)editor.addCommand(monaco.KeyMod.CtrlCmd|monaco.KeyCode.KeyS,()=>saveRef.current?.())};useEffect(()=>{const model=ref.current?.getModel();if(model)monaco.editor.setModelMarkers(model,'musp',diagnostics.map(d=>({message:d.message,severity:d.severity==='warning'?monaco.MarkerSeverity.Warning:monaco.MarkerSeverity.Error,startLineNumber:d.line||1,startColumn:d.column||1,endLineNumber:d.line||1,endColumn:(d.column||1)+1})))},[diagnostics]);return <Editor height={height} language={language} value={value} onChange={v=>onChange?.(v||'')} theme={dark?'vs-dark':'light'} onMount={onMount} options={{fontSize:12,fontFamily:'Cascadia Code, Consolas, monospace',minimap:{enabled:false},automaticLayout:true,scrollBeyondLastLine:false,wordWrap:'on',tabSize:2,readOnly,padding:{top:12},renderLineHighlight:'line'}} loading={<div className="editor-loading">正在加载编辑器…</div>}/>}
