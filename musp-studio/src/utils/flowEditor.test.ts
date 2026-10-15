import {describe,it,expect} from 'vitest';
import {flowNodeData,serializeNodeData,canRunFlow,matchingEndpoint,flowStartReason,parseObjectJson} from './flowEditor';
describe('流程编辑与发布安全',()=>{
 it('保存草稿或发布失败后禁止运行旧发布版本',()=>{expect(canRunFlow({published:true,savedPublished:true,dirty:false,busy:false,endpointId:'ep'})).toBe(true);expect(canRunFlow({published:false,savedPublished:false,dirty:false,busy:false,endpointId:'ep'})).toBe(false);expect(canRunFlow({published:false,savedPublished:true,dirty:false,busy:false,endpointId:'ep'})).toBe(false);expect(canRunFlow({published:true,savedPublished:true,dirty:true,busy:false,endpointId:'ep'})).toBe(false);expect(canRunFlow({published:true,savedPublished:true,dirty:false,busy:true,endpointId:'ep'})).toBe(false)});
 it('语音视频模板经画布编辑序列化不覆盖业务kind',()=>{for(const kind of ['VIDEO','VOICE']){const original={id:'create-call',type:'UPDATE_CALL',data:{kind,operation:'create',label:'创建通话',requestedRx:800}};const canvas=flowNodeData(original,'更新通话');expect(canvas.nodeKind).toBe('UPDATE_CALL');expect(canvas.kind).toBe(kind);expect(serializeNodeData(canvas)).toEqual({type:'UPDATE_CALL',data:original.data});}});
});
describe('运行前校验',()=>{
 const endpoint={id:'ep',name:'端点',bind:'127.0.0.1:9001',protocolId:'p',running:true};
 const ready={flowId:'f',protocolId:'p',published:true,savedPublished:true,dirty:false,busy:false,endpoint,availableEndpoints:1,remote:'127.0.0.1:9002'};
 it('唯一匹配端点才自动选择，其他协议和停止端点不能入选',()=>{
  expect(matchingEndpoint([endpoint,{...endpoint,id:'other',protocolId:'q'},{...endpoint,id:'stopped',running:false}],'p')).toBe('ep');
  expect(matchingEndpoint([endpoint,{...endpoint,id:'second'}],'p')).toBeUndefined();
  expect(matchingEndpoint([endpoint,{...endpoint,id:'second'}],'p','second')).toBe('second');
 });
 it('明确区分草稿、未选端点、地址和JSON错误',()=>{
  expect(flowStartReason(ready)).toBe('');
  expect(flowStartReason({...ready,published:false})).toContain('发布');
  expect(flowStartReason({...ready,dirty:true})).toContain('发布');
  expect(flowStartReason({...ready,endpoint:undefined,availableEndpoints:2})).toContain('选择运行端点');
  expect(flowStartReason({...ready,remote:''})).toContain('对端');
  expect(flowStartReason({...ready,remote:'127.0.0.1:99999'})).toContain('端口');
  expect(flowStartReason({...ready,variablesError:'运行变量 JSON 格式错误'})).toBe('运行变量 JSON 格式错误');
  expect(flowStartReason({...ready,endpoint:{...endpoint,protocolId:'q'}})).toContain('协议');
 });
 it('变量必须为JSON对象，不能静默接受数组或null',()=>{expect(parseObjectJson('{"device":1}','运行变量')).toEqual({device:1});expect(()=>parseObjectJson('[]','运行变量')).toThrow('对象');expect(()=>parseObjectJson('null','运行变量')).toThrow('对象')});
});
