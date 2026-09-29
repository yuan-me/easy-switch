import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import type { Bootstrap, Provider } from './types';
export const preview=!isTauri();
const base={protocol:'Responses',contextWindow:null,compactLimit:null,members:[],strategy:'Failover',headers:{},imageCompatibility:false} as const;
const providers:Provider[]=[{...base,members:[],id:'official',name:'OpenAI 官方',mode:'Official',baseUrl:'',model:''},{...base,members:[],id:'sub2api',name:'Sub2API',mode:'Api',baseUrl:'https://api.example.com/v1',model:'your-model',imageCompatibility:true,hasKey:true},{...base,members:[],id:'workspace',name:'团队工作空间',mode:'Mixed',baseUrl:'https://team.example.com/v1',model:'team-model',hasKey:true},{...base,members:[{providerId:'sub2api',weight:1,enabled:true},{providerId:'workspace',weight:1,enabled:true}],id:'route',name:'日常开发路由',mode:'Aggregate',baseUrl:'',model:'routed-model'}];
const demo:Bootstrap={providers,settings:{codexHome:'C:\\Users\\Demo\\.codex',sqliteHome:null,desktopExecutable:'',desktopAppId:null,activeProviderId:'sub2api',runtimePort:47831,scrollPositions:{},enablePageRecovery:false,theme:'system',automaticUpdates:true,automaticDownload:true},version:'1.0.0',migrationError:null,busy:false};
export async function call<T>(command:string,args?:Record<string,unknown>):Promise<T>{if(!preview)return invoke<T>(command,args);await new Promise(r=>setTimeout(r,100));if(command==='bootstrap')return structuredClone(demo) as T;if(command==='scan_sessions')return {sessions:[],warnings:[]} as T;if(command==='list_operations'||command==='discover')return [] as T;if(command==='save_scroll')return undefined as T;throw new Error('这是界面演示；请在 Easy Switch 桌面程序中执行实际操作。');}
export async function events<T>(name:string,handler:(data:T)=>void){if(preview)return ()=>{};return listen<T>(name,e=>handler(e.payload));}
export async function pickDirectory(){if(preview)throw new Error('文件选择仅在桌面程序中可用');return open({directory:true,multiple:false});}
export async function pickExecutable(){if(preview)throw new Error('文件选择仅在桌面程序中可用');return open({multiple:false,filters:[{name:'Windows 程序',extensions:['exe']}]});}
