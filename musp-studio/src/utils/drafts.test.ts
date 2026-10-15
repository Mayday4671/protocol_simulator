import {describe,it,expect,beforeEach} from 'vitest';
import {writeDraft,readDraft,removeDraft,draftKey} from './drafts';
beforeEach(()=>localStorage.clear());
describe('多协议草稿隔离',()=>{
 it('切换协议或项目后恢复各自未保存源码',()=>{writeDraft('project-a','protocol-1','<p>a</p>');writeDraft('project-a','protocol-2','<p>b</p>');writeDraft('project-b','protocol-1','<p>c</p>');expect(readDraft('project-a','protocol-1')).toBe('<p>a</p>');expect(readDraft('project-a','protocol-2')).toBe('<p>b</p>');expect(readDraft('project-b','protocol-1')).toBe('<p>c</p>');removeDraft('project-a','protocol-1');expect(readDraft('project-a','protocol-1')).toBeNull();expect(readDraft('project-b','protocol-1')).toBe('<p>c</p>');});
 it('本地标识分隔符不会碰撞并可隔离多个新建协议',()=>{expect(draftKey('a-b','c')).not.toBe(draftKey('a','b-c'));writeDraft('a','new-1','first');writeDraft('a','new-2','second');expect(readDraft('a','new-1')).toBe('first');expect(readDraft('a','new-2')).toBe('second');});
});
