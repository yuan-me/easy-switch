import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import type { Bootstrap, Provider, Session, Detail, Settings } from './types';
export const preview=!isTauri();
const base={protocol:'Responses',contextWindow:null,compactLimit:null,members:[],strategy:'Failover',headers:{},imageCompatibility:false} as const;
const providers:Provider[]=[{...base,members:[],id:'official',name:'OpenAI 官方',mode:'Official',baseUrl:'',model:''},{...base,members:[],id:'sub2api',name:'Sub2API',mode:'Api',baseUrl:'https://api.example.com/v1',model:'codex-main',imageCompatibility:true,hasKey:true},{...base,members:[],id:'workspace',name:'团队工作空间',mode:'Mixed',baseUrl:'https://team.example.com/v1',model:'team-model',hasKey:true},{...base,members:[{providerId:'sub2api',weight:1,enabled:true},{providerId:'workspace',weight:1,enabled:true}],id:'route',name:'日常开发路由',mode:'Aggregate',baseUrl:'',model:'routed-model'}];
const demo:Bootstrap={providers,settings:{codexHome:'C:\\Users\\Demo\\.codex',sqliteHome:null,desktopExecutable:'',desktopAppId:null,activeProviderId:'sub2api',runtimePort:47831,scrollPositions:{},enablePageRecovery:false,theme:'system',automaticUpdates:true,automaticDownload:true},version:'1.3.0',migrationError:null,busy:false};
const demoSessions:Session[]=['仪表盘布局调整','接口错误排查','文档整理','单元测试补充','构建流程优化','模型选择器改进','会话导出校验','更新流程确认'].map((title,i)=>({id:'demo-session-'+i,title,cwd:'D:\\Projects\\'+['codex-switch','api-service','research-notes','codex-switch','infra-tools','codex-switch','tools','ops-process'][i],provider:'sub2api',path:'demo-session-'+i+'.jsonl',updated:Math.floor(Date.now()/1000)-i*3600,archived:false,database:null}));
const demoDetail:Detail={messages:[{role:'user',text:'请调整仪表盘布局，让当前供应商和今日用量更清楚。',time:new Date().toISOString()},{role:'assistant',text:'我会将当前供应商放在左上方，用两张卡片分别展示输入和输出 Token。',time:new Date().toISOString()},{role:'user',text:'保留会话入口，配色参考 FlClash。',time:new Date().toISOString()},{role:'assistant',text:'布局已整理为供应商、用量趋势和最近会话三个区域。',time:new Date().toISOString()}],tokens:[{time:new Date().toISOString(),cumulative:true,input:128400,output:32100,cached:77040,reasoning:1200,total:160500}],truncated:false,hasEncryptedContent:false,relations:[]};
demo.modelCatalogs={sub2api:{available:['codex-main','codex-fast','codex-reasoning','model-alpha','model-beta','model-gamma','model-delta','model-epsilon','model-zeta','model-eta','model-theta','model-iota'],excluded:['model-zeta','model-eta','model-theta','model-iota'],added:['model-delta','model-epsilon'],removed:[],checkedAt:Math.floor(Date.now()/1000),error:null}};
if(preview){try{const saved=JSON.parse(localStorage.getItem('easy-switch-demo-appearance')??'null');if(saved)demo.settings={...demo.settings,...saved}}catch{/* Preview preferences are optional. */}}
export async function call<T>(command:string,args?:Record<string,unknown>):Promise<T>{if(!preview)return invoke<T>(command,args);await new Promise(r=>setTimeout(r,100));if(command==='bootstrap')return structuredClone(demo) as T;if(command==='reveal_provider_key')return 'demo-api-key' as T;if(command==='scan_sessions')return {sessions:structuredClone(demoSessions),warnings:[]} as T;if(command==='session_detail')return structuredClone(demoDetail) as T;if(command==='save_settings'){demo.settings=structuredClone(args?.settings as Settings);const {theme,accentColor,colorStyle}=demo.settings;localStorage.setItem('easy-switch-demo-appearance',JSON.stringify({theme,accentColor,colorStyle}));return undefined as T;}if(command==='usage_report'){
 const q=args?.query as {bucketStarts:number[];end:number;model:string|null};
 const names=['模型 A','模型 B','模型 C','未知模型'];
 const factor=q.model?[.5,.3,.15,.05][names.indexOf(q.model)]??0:1;
 const buckets=q.bucketStarts.map((start,i)=>{
  const end=Math.min(q.bucketStarts[i+1]??q.end,Date.now()/1000);let input=0;
  for(let hour=start;hour<end;hour+=3600){const d=new Date(hour*1000);input+=Math.round((1200+(d.getDate()%5)*350+([0,0,0,0,0,0,1,2,4,7,5,4,3,5,7,8,6,4,3,2,2,1,1,0][d.getHours()])*700)*factor)}
  return {start,input,output:Math.round(input/4),cached:Math.round(input*.6)};
 });
 const input=buckets.reduce((n,b)=>n+b.input,0),output=buckets.reduce((n,b)=>n+b.output,0),cached=buckets.reduce((n,b)=>n+b.cached,0);
 return {input,output,cached,buckets,models:names.flatMap((name,i)=>q.model&&q.model!==name?[]:[{id:name,name,input:Math.round(input*(q.model?1:[.5,.3,.15,.05][i])),output:Math.round(output*(q.model?1:[.5,.3,.15,.05][i])),cached:Math.round(cached*(q.model?1:[.5,.3,.15,.05][i]))}]),sessions:[{id:'demo-session-0',name:'仪表盘布局调整',input,output,cached}],availableModels:names,warnings:[],scannedFiles:8} as T;
 }if(command==='interrupted_operations'||command==='discover')return [] as T;if(command==='save_scroll')return undefined as T;throw new Error('这是界面演示；请在 Easy Switch 桌面程序中执行实际操作。');}
export async function events<T>(name:string,handler:(data:T)=>void){if(preview)return ()=>{};return listen<T>(name,e=>handler(e.payload));}
let pendingScan:Promise<{sessions:Session[];warnings:string[]}>|null=null;
export function scanSessions(){return pendingScan??=call<{sessions:Session[];warnings:string[]}>('scan_sessions').finally(()=>{pendingScan=null})}
export async function pickDirectory(){if(preview)throw new Error('文件选择仅在桌面程序中可用');return open({directory:true,multiple:false});}
export async function pickExecutable(){if(preview)throw new Error('文件选择仅在桌面程序中可用');return open({multiple:false,filters:[{name:'Windows 程序',extensions:['exe']}]});}
