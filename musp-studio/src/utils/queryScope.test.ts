import {describe,it,expect,vi} from 'vitest';
import {QueryClient,QueryObserver} from '@tanstack/react-query';
import {refreshAfterAction} from './queryScope';
describe('项目切换查询隔离',()=>{
 it('切换项目只刷新snapshot，不把B报文写入A缓存',async()=>{
  const client=new QueryClient({defaultOptions:{queries:{retry:false,staleTime:Infinity}}});let project='A';
  const packetFn=vi.fn(async()=>({source:project,items:[project+'-packet']}));
  const snapshots=new QueryObserver(client,{queryKey:['snapshot'],queryFn:async()=>({projectId:project})});
  const packets=new QueryObserver(client,{queryKey:['packets','A',1,'',null],queryFn:packetFn});
  let stopPackets:(()=>void)|undefined;
  const stopSnapshot=snapshots.subscribe(result=>{if(result.data?.projectId==='B')stopPackets?.()});stopPackets=packets.subscribe(()=>{});
  await new Promise(resolve=>setTimeout(resolve,0));expect(client.getQueryData(['packets','A',1,'',null])).toEqual({source:'A',items:['A-packet']});
  project='B';await refreshAfterAction(client,'project_open');
  expect(client.getQueryData(['snapshot'])).toEqual({projectId:'B'});expect(client.getQueryData(['packets','A',1,'',null])).toBeUndefined();expect(packetFn).toHaveBeenCalledTimes(1);
  stopSnapshot();stopPackets?.();client.clear();
 });
 it('项目创建和导入也清理旧scope，常规操作保留正常刷新',async()=>{for(const action of ['project_create','project_import']){const client=new QueryClient();client.setQueryData(['protocol-source','A','p'],{xml:'A'});client.setQueryData(['device-history','A','d'],[{id:'A'}]);client.setQueryData(['flow-templates'],[{id:'built-in'}]);await refreshAfterAction(client,action);expect(client.getQueryData(['protocol-source','A','p'])).toBeUndefined();expect(client.getQueryData(['device-history','A','d'])).toBeUndefined();expect(client.getQueryData(['flow-templates'])).toEqual([{id:'built-in'}]);client.clear()}});
});
