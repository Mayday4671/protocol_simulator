import {useState} from 'react';
import {Button,Input,Select,Table,Tag} from 'antd';
import {useQuery} from '@tanstack/react-query';
import {studio,type Flow,type Protocol,type WorkflowTemplate} from '../api/tauri';
import {Panel,Blank,useAction} from '../components/Shared';
import {useUI} from '../store/ui';
const templateDescriptions:Record<string,string>={
 'config-devices-5':'逐项确认 0xAA 且数量匹配后提交。全量更新不重置注册状态；通用配置区为 8B，字段按所选协议定义。',
 'config-parameters-5':'逐项确认 0xAA 且数量匹配后提交。全量更新不重置注册状态；通用配置区为 8B，字段按所选协议定义。',
 'register-no-auth':'先创建设备。仅支持 authMode=0；authMode=1 请选择认证注册模板。',
 'register-authenticated':'先创建设备并填写 deviceId；仅支持 authMode=1。认证下发须匹配端点、协议、控制端与会话；0x12 发送成功后继续 0x03 注册，认证过期则失败。',
 'heartbeat':'发送 0x31 并等待 0x32；心跳不会自动将离线设备恢复在线。',
 'video-control':'等待设备申请。设备须已注册，并配置资源池。A/B 可共用 UDP 端点；运行前检查频点和带宽测试值。calleeNumericAddress 与 deviceBAddress 须指向同一 3B 地址。',
 'voice-control':'等待设备申请。设备须已注册，并配置资源池。A/B 可共用 UDP 端点；运行前检查频点和带宽测试值。'
};
export default function TemplateLibrary({protocols,connected,onProtocol}:{protocols:Protocol[];connected:boolean;onProtocol:(id:string)=>void}){
 const [search,setSearch]=useState('');const [protocolId,setProtocolId]=useState<string>();const {perform,busy}=useAction();const setPage=useUI(s=>s.setPage);
 const query=useQuery({queryKey:['flow-templates'],queryFn:()=>studio<WorkflowTemplate[]>('template_list'),enabled:connected});
 async function install(id:string){const result=await perform<Flow>('template_install',{id,protocolId},'业务模板已复制到当前项目');if(result){useUI.getState().setFlow(result.id);setPage('flows')}}
 return <div className="template-library"><Panel title="内置业务流程模板" extra={<Input.Search placeholder="搜索模板名称" value={search} onChange={e=>setSearch(e.target.value)} style={{width:180}}/>}><Table rowKey="id" size="small" pagination={false} dataSource={query.data?.filter(t=>t.nameZh.includes(search))||[]} expandable={{expandedRowRender:t=><div><span className="muted">输入与默认变量</span><pre className="trace-view">{JSON.stringify({inputs:t.inputs,defaultVariables:t.defaultVariables},null,2)}</pre></div>}} columns={[{title:'模板名称',dataIndex:'nameZh',render:(value,t)=><div><strong>{value}</strong><small className="template-description">{templateDescriptions[t.id]||t.description}</small></div>},{title:'节点',dataIndex:'nodeCount',width:50},{title:'协议',dataIndex:'protocol',width:100},{title:'操作',width:150,render:(_v,t)=><Button loading={busy} onClick={()=>void install(t.id)}>复制到项目并编排</Button>}]} locale={{emptyText:<Blank text={query.error?String(query.error):'暂无模板'}/>}}/><div className="inline-form"><Select allowClear value={protocolId} onChange={setProtocolId} placeholder="绑定协议（默认首个发布版本）" options={protocols.filter(p=>p.published).map(p=>({value:p.id,label:p.name+' · v'+p.version}))} style={{width:'100%'}}/></div><p className="muted">配置设备、资源池和地址变量后发布运行。</p></Panel><Panel title="用户协议与版本模板"><Table rowKey="id" size="small" pagination={{pageSize:5,showSizeChanger:false}} dataSource={protocols.filter(p=>p.name.includes(search))} onRow={p=>({onClick:()=>onProtocol(p.id)})} columns={[{title:'协议名称',dataIndex:'name'},{title:'版本',dataIndex:'version',render:v=><code>v{v}</code>,width:60},{title:'状态',dataIndex:'published',render:v=><Tag color={v?'green':'orange'}>{v?'发布快照':'草稿'}</Tag>,width:90},{title:'操作',width:90,render:(_v,p)=><Button onClick={()=>onProtocol(p.id)}>编辑 XML</Button>}]} locale={{emptyText:<Blank text="导入或保存 XML 后显示协议"/>}}/><p className="muted">编辑已发布协议将保存为草稿。</p></Panel></div>;
}
