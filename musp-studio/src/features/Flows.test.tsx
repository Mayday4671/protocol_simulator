import {beforeEach,afterEach,describe,it,expect,vi} from 'vitest';
import {render,screen,fireEvent,waitFor,cleanup} from '@testing-library/react';
import {QueryClient,QueryClientProvider} from '@tanstack/react-query';
import type {Flow,Snapshot} from '../api/tauri';
import Flows from './Flows';

const perform=vi.hoisted(()=>vi.fn());
vi.mock('../api/tauri',()=>({studio:vi.fn(async()=>[])}));
vi.mock('../components/Shared',async importOriginal=>({...await importOriginal<typeof import('../components/Shared')>(),useAction:()=>({perform,busy:false,message:{warning:vi.fn(),error:vi.fn(),success:vi.fn()}})}));
vi.mock('antd',async importOriginal=>({...await importOriginal<typeof import('antd')>(),Select:({placeholder,value,onChange,options}:{placeholder:string;value?:string;onChange?:(value:string)=>void;options?:{label:string;value:string}[]})=><select aria-label={placeholder} value={value||''} onChange={event=>onChange?.(event.target.value)}><option value="">{placeholder}</option>{options?.map(option=><option key={option.value} value={option.value}>{option.label}</option>)}</select>}));
vi.mock('@xyflow/react',async importOriginal=>({...await importOriginal<typeof import('@xyflow/react')>(),ReactFlow:()=>null}));
vi.mock('../components/CodeEditor',()=>({CodeEditor:()=>null}));
const flow:Flow={id:'published-flow',nameZh:'发布流程',published:true,nodes:[{id:'start',type:'START',data:{}},{id:'end',type:'END',data:{}}],edges:[{source:'start',target:'end'}]};
const snapshot:Snapshot={projectId:'A',projects:[],systems:[],devices:[],endpoints:[{id:'ep',name:'运行端点一',running:true,bind:'127.0.0.1:9001',protocolId:'p'}],protocols:[],flows:[flow],runs:[],pending:[],calls:[],resources:[],pools:[],stats:{}};
const computedStyle=window.getComputedStyle.bind(window);
beforeEach(()=>{
 vi.spyOn(window,'getComputedStyle').mockImplementation(element=>computedStyle(element));
 Object.defineProperty(window,'matchMedia',{writable:true,value:()=>({matches:false,addListener:()=>{},removeListener:()=>{},addEventListener:()=>{},removeEventListener:()=>{},dispatchEvent:()=>false})});
 vi.stubGlobal('ResizeObserver',class{observe(){}unobserve(){}disconnect(){}});
 perform.mockReset();perform.mockResolvedValue({...flow,published:false});
});
afterEach(cleanup);
describe('流程启动入口',()=>{
 it('已发布流程修改后保存为草稿，不会继续启动旧发布版本',async()=>{
  const client=new QueryClient({defaultOptions:{queries:{retry:false}}});
  render(<QueryClientProvider client={client}><Flows data={snapshot} projectId="A" systemId=""/></QueryClientProvider>);
  fireEvent.change(screen.getByLabelText('选择已保存流程'),{target:{value:flow.id}});
  fireEvent.change(screen.getByLabelText('运行端点'),{target:{value:'ep'}});
  const start=screen.getByRole('button',{name:/启动流程/}) as HTMLButtonElement;
  expect(start.disabled).toBe(false);
  fireEvent.change(screen.getAllByDisplayValue('发布流程').find(element=>element.tagName==='INPUT')!,{target:{value:'修改后的草稿'}});
  expect(start.disabled).toBe(true);
  fireEvent.click(screen.getByRole('button',{name:/保存草稿/}));
  await waitFor(()=>expect(perform).toHaveBeenCalledWith('flow_save',expect.objectContaining({flow:expect.objectContaining({nameZh:'修改后的草稿'})}),expect.any(String)));
  await screen.findByText('草稿 · 需发布后运行');
  expect(start.disabled).toBe(true);
  fireEvent.click(start);
  expect(perform.mock.calls.some(([action])=>action==='flow_run')).toBe(false);
  client.clear();
 },30000);
});
