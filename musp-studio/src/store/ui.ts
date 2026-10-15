import {create} from 'zustand';
import {persist} from 'zustand/middleware';
export type Page='dashboard'|'protocol'|'devices'|'flows'|'monitor'|'calls'|'templates'|'settings';
interface UIState{page:Page;dark:boolean;collapsed:boolean;projectId:string;systemId:string;flowId:string;setFlow:(id:string)=>void;setPage:(page:Page)=>void;toggleTheme:()=>void;toggleSidebar:()=>void;setProject:(id:string)=>void;setSystem:(id:string)=>void}
export const useUI=create<UIState>()(persist(set=>({page:'dashboard',dark:true,collapsed:false,projectId:'',systemId:'',flowId:'',setFlow:flowId=>set({flowId}),setPage:page=>set({page}),toggleTheme:()=>set(s=>({dark:!s.dark})),toggleSidebar:()=>set(s=>({collapsed:!s.collapsed})),setProject:projectId=>set({projectId,systemId:'',flowId:''}),setSystem:systemId=>set({systemId})}),{name:'musp-ui'}));
