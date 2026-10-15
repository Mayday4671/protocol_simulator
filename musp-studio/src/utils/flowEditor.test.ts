import {describe,it,expect} from 'vitest';
import {flowNodeData,serializeNodeData,canRunFlow} from './flowEditor';
describe('流程编辑与发布安全',()=>{
 it('保存草稿或发布失败后禁止运行旧发布版本',()=>{expect(canRunFlow({published:true,savedPublished:true,dirty:false,busy:false,endpointId:'ep'})).toBe(true);expect(canRunFlow({published:false,savedPublished:false,dirty:false,busy:false,endpointId:'ep'})).toBe(false);expect(canRunFlow({published:false,savedPublished:true,dirty:false,busy:false,endpointId:'ep'})).toBe(false);expect(canRunFlow({published:true,savedPublished:true,dirty:true,busy:false,endpointId:'ep'})).toBe(false);expect(canRunFlow({published:true,savedPublished:true,dirty:false,busy:true,endpointId:'ep'})).toBe(false)});
 it('语音视频模板经画布编辑序列化不覆盖业务kind',()=>{for(const kind of ['VIDEO','VOICE']){const original={id:'create-call',type:'UPDATE_CALL',data:{kind,operation:'create',label:'创建通话',requestedRx:800}};const canvas=flowNodeData(original,'更新通话');expect(canvas.nodeKind).toBe('UPDATE_CALL');expect(canvas.kind).toBe(kind);expect(serializeNodeData(canvas)).toEqual({type:'UPDATE_CALL',data:original.data});}});
});
