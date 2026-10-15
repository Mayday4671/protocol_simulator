import {useState} from 'react';
import {afterEach,describe,it,expect,vi} from 'vitest';
import {render,screen,fireEvent,waitFor,cleanup,act} from '@testing-library/react';
import {studio,type FlowNode,type ProtocolCatalog} from '../api/tauri';
import {FlowNodeEditor} from './FlowNodeEditor';
import type {FlowNodeDraft} from '../utils/flowEditor';
vi.mock('../api/tauri',()=>({studio:vi.fn()}));
vi.mock('./PacketInspector',()=>({default:({hex}:{hex:string})=><div data-testid="inspector">{hex}</div>}));
vi.mock('antd',async original=>({...await original<typeof import('antd')>(),Select:({value,onChange,options,...props}:{value?:number;onChange?:(value:number)=>void;options?:{label:string;value:number}[];'aria-label'?:string})=><select aria-label={props['aria-label']} value={value??''} onChange={event=>onChange?.(Number(event.target.value))}><option value=""/>{options?.map(option=><option key={option.value} value={option.value}>{option.label}</option>)}</select>}));
const catalog:ProtocolCatalog={protocolId:'p',messages:[{code:149,name:'自定义遥测',responseCodes:[150],fields:[],defaultValues:{body:{power:4}},diagnostics:[]}]};
function Harness(){const [node,setNode]=useState<FlowNode>({id:'send',type:'SEND',data:{label:'遥测发送',code:149,values:{body:{power:1}}}});const [draft,setDraft]=useState<FlowNodeDraft>({});return <FlowNodeEditor projectId="A" protocolId="p" node={node} catalog={catalog} variables={{deviceId:'d'}} draft={draft} onDraft={setDraft} onChange={data=>setNode({...node,data})}/>}
afterEach(()=>{cleanup();vi.clearAllMocks()});
describe('节点字段和发送预览',()=>{
 it('历史响应仅在明确勾选后用于预览',async()=>{
  vi.mocked(studio).mockResolvedValue({missingVariables:[],diagnostics:[],expectedCodes:[],placeholderFields:[],previewOnly:true});
  render(<FlowNodeEditor projectId="A" protocolId="p" node={{id:'send',type:'SEND',data:{code:149,values:{}}}} catalog={catalog} variables={{}} response={{value:7}} draft={{}} onDraft={vi.fn()} onChange={vi.fn()}/>);
  fireEvent.click(screen.getByRole('button',{name:'预览报文'}));
  await waitFor(()=>expect(studio).toHaveBeenCalledTimes(1));
  expect(vi.mocked(studio).mock.calls[0][1]).not.toHaveProperty('response');
  fireEvent.click(screen.getByRole('checkbox',{name:'引用所选运行的响应'}));
  fireEvent.click(screen.getByRole('button',{name:'预览报文'}));
  await waitFor(()=>expect(studio).toHaveBeenLastCalledWith('flow_preview',expect.objectContaining({response:{value:7}})));
 });
 it('切换信令保留已输入字段，默认值只由明确按钮载入',()=>{
  const onChange=vi.fn();const messages=[...catalog.messages,{...catalog.messages[0],code:151,name:'另一信令',defaultValues:{body:{power:999}}}];
  render(<FlowNodeEditor projectId="A" protocolId="p" node={{id:'send',type:'SEND',data:{code:149,values:{body:{power:7}}}}} catalog={{...catalog,messages}} variables={{}} draft={{}} onDraft={vi.fn()} onChange={onChange}/>);
  fireEvent.change(screen.getByLabelText('发送信令'),{target:{value:'151'}});
  expect(onChange).toHaveBeenCalledWith(expect.objectContaining({code:151,values:{body:{power:7}}}));
 });
 it('从协议显示信令名称，并用最新字段值预览而不发送',async()=>{
  vi.mocked(studio).mockResolvedValue({hex:'95 07',decoded:{fields:[]},missingVariables:[],diagnostics:[],expectedCodes:[],placeholderFields:[],previewOnly:true});
  render(<Harness/>);expect(screen.getByText(/自定义遥测/)).toBeTruthy();
  fireEvent.change(screen.getByLabelText('字段值 JSON'),{target:{value:'{"body":{"power":7}}'}});
  fireEvent.click(screen.getByRole('button',{name:'预览报文'}));
  await waitFor(()=>expect(studio).toHaveBeenCalledWith('flow_preview',expect.objectContaining({expectedProjectId:'A',protocolId:'p',node:expect.objectContaining({data:expect.objectContaining({values:{body:{power:7}}})})})));
  expect(await screen.findByTestId('inspector')).toHaveProperty('textContent','95 07');
  expect(vi.mocked(studio).mock.calls.every(([action])=>action==='flow_preview')).toBe(true);
 },15000);
 it('缺失变量显示具体名字，错误JSON不能预览',async()=>{
  vi.mocked(studio).mockResolvedValue({missingVariables:['deviceAddress'],diagnostics:[],expectedCodes:[],placeholderFields:[],previewOnly:true});
  render(<Harness/>);fireEvent.click(screen.getByRole('button',{name:'预览报文'}));
  expect(await screen.findByText(/deviceAddress/)).toBeTruthy();expect(screen.queryByTestId('inspector')).toBeNull();
  fireEvent.change(screen.getByLabelText('字段值 JSON'),{target:{value:'{"body":'}});
  expect((screen.getByRole('button',{name:'预览报文'}) as HTMLButtonElement).disabled).toBe(true);
  expect((screen.getByLabelText('字段值 JSON') as HTMLTextAreaElement).value).toBe('{"body":');
 },15000);
 it('节点切换后丢弃尚未返回的旧预览',async()=>{
  let resolve!:(value:unknown)=>void;vi.mocked(studio).mockImplementation(()=>new Promise(done=>{resolve=done}));
  const props={projectId:'A',protocolId:'p',catalog,variables:{},draft:{},onDraft:vi.fn(),onChange:vi.fn()};
  const {rerender}=render(<FlowNodeEditor {...props} node={{id:'first',type:'SEND',data:{code:149,values:{}}}}/>);
  fireEvent.click(screen.getByRole('button',{name:'预览报文'}));
  rerender(<FlowNodeEditor {...props} node={{id:'second',type:'SEND',data:{code:149,values:{}}}}/>);
  await act(async()=>resolve({hex:'OLD',decoded:{fields:[]},missingVariables:[],diagnostics:[],expectedCodes:[],placeholderFields:[],previewOnly:true}));
  expect(screen.queryByTestId('inspector')).toBeNull();
 },15000);
});
