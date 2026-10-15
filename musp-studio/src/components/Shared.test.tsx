import {afterEach,describe,it,expect,vi} from 'vitest';
import {act,cleanup,renderHook} from '@testing-library/react';
import {QueryClient,QueryClientProvider} from '@tanstack/react-query';
import type {ReactNode} from 'react';
import {studio} from '../api/tauri';
import {useAction} from './Shared';

vi.mock('../api/tauri',()=>({studio:vi.fn(async()=>({id:'result'}))}));
vi.mock('antd',()=>({App:{useApp:()=>({message:{error:vi.fn(),success:vi.fn()}})}}));
afterEach(()=>{cleanup();vi.clearAllMocks()});
describe('操作的项目归属',()=>{
 it('旧页面回调保留渲染时的项目，不能借新snapshot修改另一项目',async()=>{
  const client=new QueryClient();client.setQueryData(['snapshot'],{projectId:'A'});
  const {result}=renderHook(()=>useAction(),{wrapper:({children}:{children:ReactNode})=><QueryClientProvider client={client}>{children}</QueryClientProvider>});
  const oldAction=result.current.perform;client.setQueryData(['snapshot'],{projectId:'B'});
  await act(async()=>{await oldAction('flow_save',{flow:{id:'flow-A'}})});
  expect(studio).toHaveBeenCalledWith('flow_save',{flow:{id:'flow-A'},expectedProjectId:'A'});
  client.clear();
 });
});
