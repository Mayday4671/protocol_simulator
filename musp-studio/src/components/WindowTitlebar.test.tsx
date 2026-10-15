import {beforeEach,describe,it,expect,vi} from 'vitest';
import {render,screen,fireEvent,waitFor,cleanup} from '@testing-library/react';
import {WindowTitlebar} from './WindowTitlebar';
const native=vi.hoisted(()=>({maximized:false,minimize:vi.fn(),close:vi.fn(),startDragging:vi.fn(),toggleMaximize:vi.fn(),isMaximized:vi.fn(),onResized:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({isTauri:()=>true}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>native}));
beforeEach(()=>{cleanup();vi.clearAllMocks();native.maximized=false;native.isMaximized.mockImplementation(async()=>native.maximized);native.toggleMaximize.mockImplementation(async()=>{native.maximized=!native.maximized});native.onResized.mockResolvedValue(()=>{});native.minimize.mockResolvedValue(undefined);native.close.mockResolvedValue(undefined);native.startDragging.mockResolvedValue(undefined)});
describe('统一桌面标题栏',()=>{
 it('窗口按钮调用真实Tauri接口并同步最大化/还原标签',async()=>{render(<WindowTitlebar><div data-window-drag>工作区</div></WindowTitlebar>);fireEvent.click(screen.getByRole('button',{name:'最小化窗口'}));expect(native.minimize).toHaveBeenCalledOnce();fireEvent.click(screen.getByRole('button',{name:'最大化窗口'}));await screen.findByRole('button',{name:'还原窗口'});expect(native.toggleMaximize).toHaveBeenCalledOnce();fireEvent.click(screen.getByRole('button',{name:'关闭窗口'}));expect(native.close).toHaveBeenCalledOnce();});
 it('品牌和空白可拖动及双击最大化，交互控件不拖动',async()=>{render(<WindowTitlebar><div data-window-drag>品牌</div><button>项目操作</button><div role="combobox"><span>项目选择</span></div></WindowTitlebar>);fireEvent.mouseDown(screen.getByText('品牌'),{button:0,detail:1});await waitFor(()=>expect(native.startDragging).toHaveBeenCalledOnce());fireEvent.mouseDown(screen.getByText('项目操作'),{button:0,detail:1});fireEvent.mouseDown(screen.getByText('项目选择'),{button:0,detail:1});expect(native.startDragging).toHaveBeenCalledOnce();fireEvent.doubleClick(screen.getByText('品牌'));await waitFor(()=>expect(native.toggleMaximize).toHaveBeenCalledOnce());fireEvent.doubleClick(screen.getByText('项目操作'));expect(native.toggleMaximize).toHaveBeenCalledOnce();});
});
