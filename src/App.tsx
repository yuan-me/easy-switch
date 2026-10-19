import {useState,useEffect,useCallback,useRef,lazy,Suspense} from 'react';
import {Plug,MessageSquare,Settings as SettingsIcon,LayoutGrid,AlertCircle,CheckCircle2,Loader2,X,Download,ArrowRight,Activity,ChartColumn} from 'lucide-react';
import {call,events} from './api';
import type {Bootstrap,Provider,Diagnostic,Operation,UpdateStatus,Progress,ModelList} from './types';
import {IconButton,Modal,Empty,type Confirm} from './components';
import {Titlebar} from './Titlebar';
import {OperationProgress} from './OperationProgress';
import {applyAppearance} from './appearance';
import {useSettingsAutosave} from './useSettingsAutosave';
import Dashboard from './Dashboard';
import {Providers,ProviderEditor} from './Providers';
const Sessions=lazy(()=>import('./Sessions'));const SettingsPage=lazy(()=>import('./SettingsPage'));
const Usage=lazy(()=>import('./Usage'));
type Page='dashboard'|'providers'|'sessions'|'usage'|'settings';const pageNames={dashboard:'仪表盘',providers:'供应商',sessions:'会话',usage:'Token 统计',settings:'设置'};
export default function App(){
 useEffect(()=>{
  const timers=new Map<HTMLElement,ReturnType<typeof setTimeout>>();
  const onScroll=(event:Event)=>{
   const element=event.target===document?document.scrollingElement:event.target;
   if(!(element instanceof HTMLElement))return;
   clearTimeout(timers.get(element));
   if(!element.dataset.scrolling)element.dataset.scrolling='true';
   timers.set(element,setTimeout(()=>{delete element.dataset.scrolling;timers.delete(element)},800));
  };
  document.addEventListener('scroll',onScroll,{capture:true,passive:true});
  return()=>{
   document.removeEventListener('scroll',onScroll,true);
   timers.forEach((timer,element)=>{clearTimeout(timer);delete element.dataset.scrolling});
  };
 },[]);
 const [visited,setVisited]=useState(false);
 const promptedUpdate=useRef<string|null>(null);
 const [requestedSession,setRequestedSession]=useState<string|undefined>();
 const [data,setData]=useState<Bootstrap|null>(null),[page,setPage]=useState<Page>('dashboard'),[busy,setBusy]=useState(false),[loading,setLoading]=useState(true),[notice,setNotice]=useState<{text:string;error:boolean}|null>(null),[editor,setEditor]=useState<Provider|null>(null),[confirm,setConfirm]=useState<Confirm|null>(null),[diagnosis,setDiagnosis]=useState<{name:string;rows:Diagnostic[]}|null>(null),[progress,setProgress]=useState<Progress|null>(null),[update,setUpdate]=useState<UpdateStatus>({state:'idle'}),[ops,setOps]=useState<Operation[]>([]);
 const autosave=useSettingsAutosave(data?.settings??null,patch=>setData(current=>current?{...current,settings:{...current.settings,...patch}}:current));
 const appearance=autosave.settings;
 useEffect(()=>{if(!notice)return;const timer=setTimeout(()=>setNotice(null),10000);return()=>clearTimeout(timer)},[notice]);
 const notify=useCallback((text:string,error=false)=>setNotice({text,error}),[]);
 async function refresh(){const next=await call<Bootstrap>('bootstrap');setData(next);if(next.migrationError)notify('旧数据导入未完成：'+next.migrationError,true);setOps(await call<Operation[]>('interrupted_operations'))}
 const run=useCallback(async(fn:()=>Promise<void>)=>{setBusy(true);setProgress(null);try{await fn()}catch(e){notify(String(e),true);try{setOps(await call<Operation[]>('interrupted_operations'))}catch{/* Keep the original operation error visible. */}}finally{setBusy(false);setProgress(null)}},[notify]);
 useEffect(()=>{refresh().catch(e=>notify(String(e),true)).finally(()=>setLoading(false));const disposers=[events<Progress>('operation-progress',setProgress),events<UpdateStatus>('update-status',setUpdate),events<Record<string,ModelList>>('models-updated',modelCatalogs=>setData(current=>current?{...current,modelCatalogs}:current))];return()=>{disposers.forEach(p=>p.then(fn=>fn()))}},[]);
 useEffect(()=>{const settings=appearance??{theme:'system' as const};const media=matchMedia('(prefers-color-scheme: dark)');const apply=()=>applyAppearance(settings);apply();media.addEventListener('change',apply);return()=>media.removeEventListener('change',apply)},[appearance?.theme,appearance?.accentColor,appearance?.colorStyle]);
 useEffect(()=>{if(page==='sessions')setVisited(true)},[page]);
 useEffect(()=>{
  if(update.state!=='ready'||busy||confirm||editor||diagnosis)return;
  const version=update.version??'';
  if(promptedUpdate.current===version)return;
  promptedUpdate.current=version;
  confirmUpdate();
 },[update.state,update.version,busy,confirm,editor,diagnosis]);
 function confirmUpdate(){setConfirm({title:'关闭 Codex 并更新 Easy Switch',text:'确认后将正常关闭 Codex，然后退出 Easy Switch 并安装更新。请先保存工作。',action:'关闭 Codex 并更新',run:async()=>{await call('install_update')}})}
 const current=data?.providers.find(p=>p.id===data.settings.activeProviderId);const nav=[{id:'dashboard' as const,icon:LayoutGrid},{id:'providers' as const,icon:Plug},{id:'usage' as const,icon:ChartColumn},{id:'sessions' as const,icon:MessageSquare},{id:'settings' as const,icon:SettingsIcon}];
 function switchTo(p:Provider){setConfirm({title:'切换到 '+p.name,text:'将正常退出 Codex，临时备份配置与会话，修复供应商关联，然后重新启动。成功后自动清理操作副本。正在生成的任务将被中断；请先保存工作。',action:'切换并重启',run:async()=>{const message=await call<string>('switch_provider',{id:p.id});await refresh();notify(message==='Codex 已启动'?'切换成功':message)}})}
 function diagnose(p:Provider,network:boolean){const action=async()=>setDiagnosis({name:p.name,rows:await call<Diagnostic[]>('doctor',{id:p.id,network})});if(network)setConfirm({title:'执行联网诊断',text:'最多发送 5 个合成请求，可能产生少量费用。不发送本地会话，也不执行返回的工具。',action:'开始诊断',run:action});else run(action)}
 return <div className="app-shell"><Titlebar ready={!loading} theme={appearance?.theme??'system'} onError={text=>notify(text,true)}/><aside className="sidebar"><nav aria-label="主导航">{nav.map(({id,icon:Icon})=><button key={id} className={page===id?'nav-item active':'nav-item'} aria-current={page===id?'page':undefined} onClick={()=>{if(id==='sessions')setRequestedSession(undefined);setPage(id)}}><Icon size={23}/><span>{pageNames[id]}</span></button>)}</nav></aside><main>{notice&&<div className={'notice '+(notice.error?'error':'')} role={notice.error?'alert':'status'}>{notice.error?<AlertCircle size={18}/>:<CheckCircle2 size={18}/>}<span>{notice.text}</span><IconButton label="关闭提示" onClick={()=>setNotice(null)}><X size={16}/></IconButton></div>}{update.state==='ready'&&<div className="update-banner"><Download size={17}/><span>Easy Switch {update.version} 已下载，准备安装</span><button onClick={()=>setPage('settings')}>查看更新 <ArrowRight size={15}/></button></div>}
 {autosave.error&&<div className="notice error" role="alert"><AlertCircle size={18}/><span>设置保存失败：{autosave.error}</span><button onClick={()=>void autosave.retry()}>重试保存</button><button onClick={()=>setPage('settings')}>检查设置</button></div>}
 {data?.cleanupError&&<div className="notice error" role="alert"><AlertCircle size={18}/><span>旧操作副本清理未完成，下次启动会重试：{data.cleanupError}</span></div>}
 {ops.map(o=><div className="notice error" role="alert" key={o.id+(o.legacy?'old':'')}><AlertCircle size={18}/><span>上次操作中断：{o.description}。请先恢复再继续。</span><button disabled={busy} onClick={()=>setConfirm({title:'恢复中断操作',text:'将正常退出 Codex，校验文件并恢复到操作前的状态。检测到外部修改时停止恢复；成功后自动清理临时副本。',action:'检查并恢复',run:async()=>{const message=await call<string>('restore_operation',{id:o.id,legacy:o.legacy});await refresh();notify(message)}})}>恢复中断操作</button></div>)}
 {loading?<div className="loading"><Loader2 className="spin"/>读取本地配置…</div>:!data?<Empty title="无法读取本地配置"><button onClick={()=>run(refresh)}>重试</button></Empty>:<Suspense fallback={<div className="loading"><Loader2 className="spin"/>加载界面…</div>}>
 {page==='dashboard'&&<Dashboard current={current} modelCatalog={current?data.modelCatalogs?.[current.id]:undefined} codexHome={data.settings.codexHome} busy={busy} onProviders={()=>setPage('providers')} onModels={()=>current&&['Api','Mixed'].includes(current.mode)?setEditor(current):setPage('providers')} onSession={id=>{setRequestedSession(id);setPage('sessions')}} onSessions={()=>{setRequestedSession(undefined);setPage('sessions')}} onRefresh={()=>run(refresh)}/>}
 <div hidden={page!=='providers'}><Providers modelCatalogs={data.modelCatalogs} providers={data.providers} current={data.settings.activeProviderId} busy={busy} onEdit={setEditor} onSwitch={switchTo} onDiagnose={diagnose} onDelete={p=>setConfirm({title:'删除供应商配置',text:`删除“${p.name}”只移除此配置，不删除 Codex 会话。`,action:'删除配置',danger:true,run:async()=>{await call('delete_provider',{id:p.id});await refresh()}})}/></div>
 <div className="session-host" hidden={page!=='sessions'}>{(visited||page==='sessions')&&<Sessions requestedId={requestedSession} visible={page==='sessions'} settings={data.settings} current={current} busy={busy} run={run} notify={notify} confirm={setConfirm}/>}</div>
 {page==='usage'&&<Usage onSession={id=>{setRequestedSession(id);setPage('sessions')}}/>}
 {page==='settings'&&<SettingsPage version={data.version} settings={autosave.settings!} busy={busy} update={update} run={run} notify={notify} onChange={autosave.change} saveStatus={autosave.error?'未保存':autosave.status} onCheck={()=>run(async()=>{const v=await call<UpdateStatus>('check_update');setUpdate(v);if(v.state==='available'&&data.settings.automaticDownload)setUpdate(await call<UpdateStatus>('download_update'))})} onDownload={()=>run(async()=>setUpdate(await call<UpdateStatus>('download_update')))} onInstall={confirmUpdate}/>}</Suspense>}
 {busy&&<OperationProgress progress={progress} onError={text=>notify(text,true)}/>}</main>
 {editor&&data&&<ProviderEditor modelCatalog={data.modelCatalogs?.[editor.id]} provider={editor} all={data.providers} onClose={()=>setEditor(null)} onSave={async(p,key,modelSelection)=>{await call('save_provider',{provider:p,key,modelSelection:modelSelection??null});if(['Api','Mixed'].includes(p.mode))await call('acknowledge_models',{id:p.id});await refresh();setEditor(null);notify(p.id===data.settings.activeProviderId?'配置已保存，重新应用后生效':'配置已保存')}}/>}
 {confirm&&<Modal title={confirm.title} onClose={()=>{if(!busy)setConfirm(null)}}><p className="confirm-copy">{confirm.text}</p><div className="modal-footer"><button onClick={()=>setConfirm(null)} disabled={busy}>取消</button><button className={confirm.danger?'danger':'primary'} disabled={busy} onClick={()=>{const action=confirm.run;setConfirm(null);run(action)}}>{confirm.action}</button></div></Modal>}
 {diagnosis&&<Modal title={diagnosis.name+' · 诊断报告'} onClose={()=>setDiagnosis(null)}><div className="diagnostics">{diagnosis.rows.map((d,i)=><div key={i}><span className={'diagnostic-icon '+d.state}>{d.state==='passed'?<CheckCircle2 size={18}/>:d.state==='failed'?<AlertCircle size={18}/>:<Activity size={18}/>}</span><div><strong>{d.name}</strong><p>{d.detail}</p></div><small>{({passed:'通过',failed:'失败',warning:'注意',untested:'未测'})[d.state]}</small></div>)}</div><div className="modal-footer"><button className="primary" onClick={()=>setDiagnosis(null)}>完成</button></div></Modal>}</div>
}
