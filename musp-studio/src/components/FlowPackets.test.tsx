import {beforeEach,afterEach,describe,it,expect,vi} from 'vitest';
import {render,screen,fireEvent,waitFor,cleanup} from '@testing-library/react';
import {QueryClient,QueryClientProvider} from '@tanstack/react-query';
import {studio} from '../api/tauri';
import {FlowPackets} from './FlowPackets';
vi.mock('../api/tauri',()=>({studio:vi.fn()}));
vi.mock('./PacketInspector',()=>({default:({hex}:{hex:string})=><div>{hex}</div>}));
beforeEach(()=>{Object.defineProperty(window,'matchMedia',{writable:true,value:()=>({matches:false,addListener:()=>{},removeListener:()=>{},addEventListener:()=>{},removeEventListener:()=>{},dispatchEvent:()=>false})})});
afterEach(()=>{cleanup();vi.clearAllMocks()});
describe('流程实际报文隔离',()=>{
 it('未启动时不查询报文，切换项目和run后立即清除旧报文',async()=>{
  const client=new QueryClient({defaultOptions:{queries:{retry:false}}});
  vi.mocked(studio).mockImplementation(async(_action,args)=>({items:[{id:'a',runId:'run-A',nodeId:'send',timestamp:1,direction:'TX',hex:'A1',length:1,status:'已解析',remote:'127.0.0.1:3'},{id:'b',runId:'run-B',nodeId:'send',timestamp:2,direction:'RX',hex:'B2',length:1,status:'已解析',remote:'127.0.0.1:4'}].filter(packet=>packet.runId===args?.runId),total:1}));
  const view=(projectId:string,runId?:string)=><QueryClientProvider client={client}><FlowPackets projectId={projectId} flowId="flow" runId={runId}/></QueryClientProvider>;
  const {rerender}=render(view('A'));expect(studio).not.toHaveBeenCalled();
  rerender(view('A','run-A'));await screen.findByText('A1');
  expect(studio).toHaveBeenCalledWith('packet_query',expect.objectContaining({runId:'run-A',expectedProjectId:'A'}));
  rerender(view('B','run-B'));expect(screen.queryByText('A1')).toBeNull();
  await screen.findByText('B2');await waitFor(()=>expect(studio).toHaveBeenCalledWith('packet_query',expect.objectContaining({runId:'run-B',expectedProjectId:'B'})));
  client.clear();
 },15000);
 it('当前端点可在没有run时显示未匹配坏包，不冒充本次运行',async()=>{
  const client=new QueryClient({defaultOptions:{queries:{retry:false}}});
  vi.mocked(studio).mockResolvedValue({items:[{id:'bad',runId:null,nodeId:null,timestamp:3,direction:'RX',hex:'FF EE',length:2,status:'解析失败',remote:'127.0.0.1:4'}],total:1});
  render(<QueryClientProvider client={client}><FlowPackets projectId="A" flowId="flow" endpointId="ep"/></QueryClientProvider>);
  expect(studio).not.toHaveBeenCalled();fireEvent.click(screen.getByText('当前端点'));
  await screen.findByText('FF EE');expect(studio).toHaveBeenCalledWith('packet_query',{expectedProjectId:'A',offset:0,limit:100,endpointId:'ep'});
  expect(screen.getByLabelText('端点收发')).toBeTruthy();expect(screen.queryByLabelText('本次收发')).toBeNull();client.clear();
 },15000);
});
