import type {Query,QueryClient} from '@tanstack/react-query';
const projectActions=new Set(['project_open','project_create','project_import']);
const scopedKeys=new Set(['packets','protocol-source','flow-catalog','device-history','settings']);
const scoped=(query:Query)=>scopedKeys.has(String(query.queryKey[0]));
export function changesProject(action:string):boolean{return projectActions.has(action)}
export async function cancelProjectQueries(client:QueryClient):Promise<void>{await client.cancelQueries({predicate:query=>scoped(query)||query.queryKey[0]==='snapshot'})}
export async function refreshAfterAction(client:QueryClient,action:string):Promise<void>{
 if(!changesProject(action)){await client.invalidateQueries();return}
 await cancelProjectQueries(client);
 client.removeQueries({predicate:scoped});
 await client.invalidateQueries({queryKey:['snapshot'],exact:true});
}
