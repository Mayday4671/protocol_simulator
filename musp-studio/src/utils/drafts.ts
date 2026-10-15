export function draftKey(projectId:string,documentId:string):string{return 'musp-draft:'+JSON.stringify([projectId,documentId])}
export function readDraft(projectId:string,documentId:string):string|null{return localStorage.getItem(draftKey(projectId,documentId))??localStorage.getItem('musp-draft-'+projectId+'-'+documentId)}
export function writeDraft(projectId:string,documentId:string,xml:string):void{localStorage.setItem(draftKey(projectId,documentId),xml)}
export function removeDraft(projectId:string,documentId:string):void{localStorage.removeItem(draftKey(projectId,documentId));localStorage.removeItem('musp-draft-'+projectId+'-'+documentId)}
export interface ProtocolTab{key:string;id?:string;name:string}
export function readTabs(projectId:string):ProtocolTab[]{try{const parsed=JSON.parse(localStorage.getItem('musp-tabs:'+projectId)||'[]');if(Array.isArray(parsed)&&parsed.length&&parsed.every(t=>typeof t.key==='string'&&typeof t.name==='string'))return parsed}catch{/* Invalid persisted metadata cannot block the editor. */}return [{key:'seed',name:'MUSP 协议种子'}]}
