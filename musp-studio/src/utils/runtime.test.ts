import {describe,it,expect} from 'vitest';
import {runControls,statusLabel} from './runtime';
describe('运行状态与控制',()=>{
 it('活动实例可暂停和停止，暂停实例可继续单步或停止',()=>{expect(runControls('WAITING')).toEqual(['pause','stop']);expect(runControls('RUNNING')).toEqual(['pause','stop']);expect(runControls('PAUSED')).toEqual(['resume','step','stop']);});
 it('终态不可再控制，所有后端运行状态均有中文',()=>{for(const status of ['COMPLETED','FAILED','CANCELLED','INTERRUPTED'])expect(runControls(status)).toEqual([]);for(const status of ['RUNNING','WAITING','PAUSED','COMPLETED','FAILED','CANCELLED','INTERRUPTED'])expect(statusLabel(status)).toMatch(/[\u4e00-\u9fff]/);});
});
