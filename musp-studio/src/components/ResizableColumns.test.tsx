import {beforeEach,describe,it,expect} from 'vitest';
import {render,screen,fireEvent,cleanup} from '@testing-library/react';
import {ResizableColumns} from './ResizableColumns';
beforeEach(()=>{cleanup();localStorage.clear()});
describe('工作区分栏',()=>{
 it('键盘调整宽度后按项目和页面恢复，其他页面不受影响',()=>{const first=render(<ResizableColumns projectId="p" page="protocol" left={{initial:220,min:180,max:500}}><div>left</div><div>right</div></ResizableColumns>);const separator=screen.getByRole('separator');fireEvent.keyDown(separator,{key:'ArrowRight'});expect(separator.getAttribute('aria-valuenow')).toBe('230');first.unmount();const next=render(<ResizableColumns projectId="p" page="protocol" left={{initial:220,min:180,max:500}}><div/></ResizableColumns>);expect(screen.getByRole('separator').getAttribute('aria-valuenow')).toBe('230');next.unmount();render(<ResizableColumns projectId="p" page="devices" left={{initial:220,min:180,max:500}}><div/></ResizableColumns>);expect(screen.getByRole('separator').getAttribute('aria-valuenow')).toBe('220');});
 it('宽度限制与双击复位保护可用布局',()=>{render(<ResizableColumns projectId="p" page="monitor" left={{initial:220,min:210,max:230}}><div/></ResizableColumns>);const separator=screen.getByRole('separator');fireEvent.keyDown(separator,{key:'ArrowLeft',shiftKey:true});expect(separator.getAttribute('aria-valuenow')).toBe('210');fireEvent.doubleClick(separator);expect(separator.getAttribute('aria-valuenow')).toBe('220');fireEvent.keyDown(separator,{key:'ArrowRight',shiftKey:true});expect(separator.getAttribute('aria-valuenow')).toBe('230');});
});
