import {describe,it,expect} from 'vitest';
import {byteRange,findFieldAtByte,flattenFields,formatMessageCode} from './packet';
import type {FieldSpan} from '../api/tauri';
const child:FieldSpan={path:'body.items[0].value',labelZh:'值',startOffset0:23,byteLength:2,rawHex:'FF FE',decimal:-2};
const parent:FieldSpan={path:'body.items[0]',labelZh:'数组项',startOffset0:23,byteLength:2,rawHex:'FF FE',children:[child]};
describe('HEX 与字段定位',()=>{
 it('未知或损坏报文的null信令码安全显示，零码值不丢失',()=>{expect(formatMessageCode(null)).toBe('—');expect(formatMessageCode(undefined)).toBe('—');expect(formatMessageCode(0)).toBe('00');expect(formatMessageCode(255)).toBe('FF')});
 it('点击字节定位最深层字段，父子范围相同也选择子字段',()=>{expect(findFieldAtByte([parent],24)?.path).toBe(child.path);expect(findFieldAtByte([parent],25)).toBeUndefined()});
 it('一开始位置与零偏移切换保留长度',()=>{expect(byteRange(child,true)).toBe('24–25');expect(byteRange(child,false)).toBe('23–24');expect(byteRange({...child,byteLength:1},true)).toBe('24')});
 it('展开字段保持后端有符号解释和原始HEX',()=>{expect(flattenFields([parent])[1]).toEqual(child);expect(flattenFields([parent])[1].decimal).toBe(-2)});
});
