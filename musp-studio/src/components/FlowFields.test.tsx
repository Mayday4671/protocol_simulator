import {afterEach,describe,it,expect,vi} from 'vitest';
import {render,screen,fireEvent,cleanup} from '@testing-library/react';
import {FlowFields,readFieldValue,writeFieldValue,type EditableCatalogField} from './FlowFields';
afterEach(cleanup);
describe('按协议信令录入字段',()=>{
 it('将模板变量替换为十进制整数，8 B 整数保持 HEX 字符串',()=>{
  const fields:EditableCatalogField[]=[{path:'body.value',key:'value',label:'输出值',kind:'field',required:true,valueKind:'uint',bytes:1},{path:'body.large',key:'large',label:'长整数',kind:'field',required:true,valueKind:'uint',bytes:8}];
  const change=vi.fn();render(<FlowFields fields={fields} values={{body:{value:'${body.value}',large:'${body.large}'}}} onChange={change}/>);
  fireEvent.change(screen.getByLabelText('输出值（body.value）'),{target:{value:'42'}});
  expect(change).toHaveBeenLastCalledWith({body:{value:42,large:'${body.large}'}});
  fireEvent.change(screen.getByLabelText('输出值（body.value）'),{target:{value:'0x42'}});
  expect(change).toHaveBeenLastCalledWith({body:{value:'42',large:'${body.large}'}});
  fireEvent.change(screen.getByLabelText('长整数（body.large）'),{target:{value:'FF FF FF FF FF FF FF FF'}});
  expect(change).toHaveBeenLastCalledWith({body:{value:'${body.value}',large:'FF FF FF FF FF FF FF FF'}});
 });
 it('编辑普通字段保留其他字段，固定及自动字段只读',()=>{
  const fields:EditableCatalogField[]=[{path:'body.power',key:'power',label:'功率',kind:'field',required:true,valueKind:'uint',bytes:1},{path:'header.magic',key:'magic',label:'同步头',kind:'field',required:false,fixed:true,defaultValue:'AB CD'},{path:'header.msgCode',key:'msgCode',label:'信令码',kind:'field',required:false,binding:'message.code'}];
  const change=vi.fn();render(<FlowFields fields={fields} values={{body:{power:3,other:8}}} code={149} onChange={change}/>);
  fireEvent.change(screen.getByLabelText('功率（body.power）'),{target:{value:'7'}});
  expect(change).toHaveBeenLastCalledWith({body:{power:7,other:8}});
  expect((screen.getByLabelText('同步头（header.magic）') as HTMLInputElement).disabled).toBe(true);
  expect((screen.getByLabelText('信令码（header.msgCode）') as HTMLInputElement).disabled).toBe(true);
 });
 it('兼容已有平铺头字段，嵌套路径按原位置写回',()=>{
  const field={path:'header.srcAddr',key:'srcAddr'};const values={srcAddr:'00 00 01',body:{nested:{value:9}}};
  expect(readFieldValue(values,field)).toBe('00 00 01');expect(writeFieldValue(values,field,'00 00 02')).toEqual({srcAddr:'00 00 02',body:{nested:{value:9}}});
  expect(writeFieldValue(values,{path:'body.nested.value',key:'value'},10)).toEqual({srcAddr:'00 00 01',body:{nested:{value:10}}});
  expect(values.body.nested.value).toBe(9);
 });
});
