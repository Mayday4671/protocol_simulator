import {describe,it,expect} from 'vitest';
import {renderToStaticMarkup} from 'react-dom/server';
import {QueryClient,QueryClientProvider} from '@tanstack/react-query';
import {App} from 'antd';
import TemplateLibrary from './TemplateLibrary';

describe('真实内置模板摘要契约',()=>{
 it('无需完整flow也能显示后端nameZh和nodeCount',()=>{
  const client=new QueryClient();
  client.setQueryData(['flow-templates'],[{id:'heartbeat',nameZh:'心跳',description:'发送与等待真实心跳',kind:'WORKFLOW',protocol:'MUSP v0.8',nodeCount:3,inputs:{},defaultVariables:{deviceId:''}}]);
  const html=renderToStaticMarkup(<QueryClientProvider client={client}><App><TemplateLibrary protocols={[]} connected={false} onProtocol={()=>{}}/></App></QueryClientProvider>);
  expect(html).toContain('心跳');expect(html).toContain('MUSP v0.8');expect(html).toContain('发送 0x31 并等待 0x32');
 });
});
