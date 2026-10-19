import {useEffect,useState} from 'react';
import {ArrowDown,ArrowUp,Box,ChartColumn,Check,ChevronRight,Database,FileText,Loader2,MessageSquare,Percent,RefreshCw,Settings2} from 'lucide-react';
import {call,scanSessions} from './api';
import {modes,type ModelList,type Provider,type Session,type UsageReport} from './types';
import {dashboardRanges} from './usageRange';
const badge=new URL('./assets/provider-badge.png',import.meta.url).href;
const wash=new URL('./assets/provider-wash.png',import.meta.url).href;

const compact=new Intl.NumberFormat('en',{notation:'compact',maximumFractionDigits:2});
const full=new Intl.NumberFormat('zh-CN');
const sum=(v:{input:number;output:number})=>v.input+v.output;
type Scan={sessions:Session[];warnings:string[]};
type Snapshot={today:UsageReport|null;week:UsageReport|null;scan:Scan|null;errors:string[]};

function HourChart({report,metric}:{report:UsageReport;metric:'input'|'output'}){
 const buckets=report.buckets.filter(b=>b.start*1000<=Date.now());
 const max=Math.max(1,...buckets.map(b=>b[metric]));
 const points=buckets.map((b,i)=>`${i/Math.max(1,buckets.length-1)*240},${90-b[metric]/max*76}`).join(' ');
 return <svg className="hour-chart" viewBox="0 0 240 100" role="img" aria-label={`今日每小时${metric==='input'?'输入':'输出'} Token 趋势`}>
  {buckets.length>1&&<polygon points={`0,100 ${points} 240,100`} fill="var(--tint)"/>}
  <polyline points={points} fill="none" stroke="var(--accent)" strokeWidth="2.5" strokeLinejoin="round"/>
  {buckets.map((b,i)=><circle key={b.start} cx={i/Math.max(1,buckets.length-1)*240} cy={90-b[metric]/max*76} r="2.5" fill="var(--accent)"><title>{new Date(b.start*1000).getHours()}:00 · {full.format(b[metric])} Token</title></circle>)}
 </svg>;
}

export default function Dashboard({current,modelCatalog,codexHome,busy,onProviders,onModels,onSession,onSessions,onRefresh}:{current?:Provider;modelCatalog?:ModelList;codexHome:string;busy:boolean;onProviders:()=>void;onModels:()=>void;onSession:(id:string)=>void;onSessions:()=>void;onRefresh:()=>Promise<void>}){
 const [revision,setRevision]=useState(0),[loading,setLoading]=useState(true);
 const [snapshot,setSnapshot]=useState<Snapshot>({today:null,week:null,scan:null,errors:[]});
 useEffect(()=>{
  let active=true;setLoading(true);
  const ranges=dashboardRanges();
  const report=(range:typeof ranges.week)=>call<UsageReport>('usage_report',{query:{bucketStarts:range.bucketStarts,end:range.end.getTime()/1000,model:null}});
  Promise.allSettled([report(ranges.today),report(ranges.week),scanSessions()]).then(([today,week,scan])=>{
   if(!active)return;
   const errors:string[]=[];
   for(const [label,result] of [['今日用量',today],['最近七天用量',week],['最近会话',scan]] as const)if(result.status==='rejected')errors.push(`${label}读取失败：${String(result.reason)}`);
   setSnapshot({today:today.status==='fulfilled'?today.value:null,week:week.status==='fulfilled'?week.value:null,scan:scan.status==='fulfilled'?scan.value:null,errors});setLoading(false);
  });
  return()=>{active=false};
 },[revision,codexHome]);
 useEffect(()=>{const id=setInterval(()=>{if(!document.hidden&&!busy)setRevision(v=>v+1)},60000);return()=>clearInterval(id)},[busy]);
 const {today,week,scan,errors}=snapshot;
 const todayAvailable=!!today?.scannedFiles,weekAvailable=!!week?.scannedFiles;
 const warnings=[...(today?.warnings??[]),...(week?.warnings??[]),...(scan?.warnings??[])];
 const recent=[...(scan?.sessions??[])].sort((a,b)=>b.updated-a.updated).slice(0,4);
 const peak=Math.max(1,...(week?.buckets.map(sum)??[]));
 const added=modelCatalog?.added.filter(id=>!modelCatalog.excluded.includes(id))??[];
 const available=modelCatalog?.available??[];
 const selected=available.filter(id=>!modelCatalog?.excluded.includes(id));
 const modelApplicable=current&&['Api','Mixed'].includes(current.mode);
 const display=(value:number|null|undefined)=>loading?'…':value==null?'暂无数据':compact.format(value);
 return <section className="page dashboard-page" aria-busy={loading}>
  <div className="page-heading"><div><h1>仪表盘</h1></div><div className="dashboard-heading-actions"><time>{new Date().toLocaleDateString('zh-CN',{year:'numeric',month:'long',day:'numeric'})}</time><button disabled={loading||busy} onClick={()=>{setRevision(v=>v+1);void onRefresh()}}>{loading?<Loader2 size={17} className="spin"/>:<RefreshCw size={17}/>}刷新</button></div></div>
  {errors.length>0&&<div className="warning-box" role="alert">{errors.map(e=><p key={e}>{e}</p>)}<button onClick={()=>setRevision(v=>v+1)} disabled={loading}>重试</button></div>}
  {warnings.length>0&&<details className="warning-box"><summary>部分本地记录未能读取（{new Set(warnings).size} 项）</summary>{[...new Set(warnings)].map(w=><p key={w}>{w}</p>)}</details>}
  <div className="dashboard-top">
   <article className="dashboard-card current-provider"><img className="provider-wash" src={wash} alt="" aria-hidden="true"/><div className="dashboard-card-label"><Box size={22}/><span>当前供应商</span></div><img className="provider-badge" src={badge} alt="" aria-hidden="true"/>
    <h2>{current?.name??'尚未选择供应商'}</h2><div className="provider-pills">{current?<><span>{modes[current.mode]}</span>{current.mode!=='Official'&&<span>{current.protocol==='Responses'?'Responses':'Chat Completions'}</span>}</>:<span>首次使用</span>}</div>
    <div className="dashboard-provider-fields"><div><span>默认模型</span><strong title={current?.model}>{current?current.model||'官方默认模型':'未配置'}</strong></div><div><span>{current?.mode==='Aggregate'?'路由成员':'接口地址'}</span><strong title={current?.baseUrl}>{!current?'未配置':current.mode==='Official'?'ChatGPT 账号登录':current.mode==='Aggregate'?`${current.members.filter(m=>m.enabled).length} 个已启用`:current.baseUrl.replace(/^https?:\/\//,'')}</strong></div></div>
    <button className="primary" onClick={onProviders}><Settings2 size={16}/>{current?'管理供应商':'添加供应商'}</button>
   </article>
   {(['input','output'] as const).map(metric=><article className="dashboard-card token-card" key={metric}><div className="dashboard-card-label">{metric==='input'?<ArrowUp size={21}/>:<ArrowDown size={21}/>}<span>今日{metric==='input'?'输入':'输出'} Token</span></div><strong className="token-value" data-testid={'dashboard-'+metric} title={todayAvailable?full.format(today![metric]):undefined}>{display(todayAvailable?today![metric]:null)}</strong><p className="muted">按小时汇总</p>{todayAvailable?<HourChart report={today!} metric={metric}/>:<div className="dashboard-no-data">{loading?'正在读取本地记录…':'暂无 Token 记录'}</div>}</article>)}
  </div>
  <div className="dashboard-bottom">
   <article className="dashboard-card dashboard-trend"><div className="dashboard-card-label"><ChartColumn size={22}/><h2>最近 7 天用量</h2></div><div className="trend-summary"><span>总计 <strong>{display(weekAvailable?sum(week!):null)}</strong>{weekAvailable?' Token':''}</span><div className="usage-legend"><span><i/>输入</span><span><i/>输出</span></div></div>
    {weekAvailable?<div className="usage-plot"><div className="usage-grid" aria-hidden="true">{[1,.5,0].map(n=><div key={n} style={{top:(1-n)*100+'%'}}><span>{compact.format(peak*n)}</span></div>)}</div><div className="usage-columns">{week!.buckets.map(b=>{const value=sum(b),date=new Date(b.start*1000);return <div className="usage-column" key={b.start} tabIndex={0} role="img" aria-label={`${date.toLocaleDateString('zh-CN')}，输入 ${full.format(b.input)}，输出 ${full.format(b.output)} Token`}><div className="usage-stack" style={{height:`${value/peak*100}%`}}><small>{compact.format(value)}</small><i className="usage-output" style={{height:`${value?b.output/value*100:0}%`}}/><i className="usage-input" style={{height:`${value?b.input/value*100:0}%`}}/></div><span className="usage-tick">{date.getMonth()+1}.{String(date.getDate()).padStart(2,'0')}</span></div>})}</div></div>:<div className="dashboard-no-data">{loading?'正在汇总…':'最近 7 天暂无 Token 记录'}</div>}
   </article>
   <article className="dashboard-card model-summary"><div className="dashboard-card-label"><RefreshCw size={21}/><h2>模型同步</h2><button className="icon-button" aria-label="查看供应商模型" onClick={onModels} disabled={busy}><ChevronRight size={20}/></button></div>{modelApplicable?<><div className="model-counts"><div><Box size={23}/><span><strong>{modelCatalog?available.length:'—'}</strong>可用模型</span></div><div><Check size={23}/><span><strong>{modelCatalog?selected.length:'—'}</strong>已选模型</span></div></div>{modelCatalog?.error?<p className="warning-box" role="status">模型同步失败：{modelCatalog.error}</p>:added.length||modelCatalog?.removed.length?<button className="model-change" onClick={onModels} disabled={busy}>新增 {added.length} 个{modelCatalog?.removed.length?` · 移除 ${modelCatalog.removed.length} 个`:''}<ChevronRight size={17}/></button>:<p className="model-sync-state">{modelCatalog?.checkedAt?'模型列表已同步':'未检测'}</p>}<div className="model-summary-footer"><span>上次同步 {modelCatalog?.checkedAt?new Date(modelCatalog.checkedAt*1000).toLocaleTimeString('zh-CN',{hour:'2-digit',minute:'2-digit'}):'未检测'}</span><button className="text-button" onClick={onModels} disabled={busy}>管理模型 <ChevronRight size={15}/></button></div></>:<div className="dashboard-no-data">{!current?'选择供应商后查看模型':current.mode==='Official'?'官方模型由 Codex 管理':'模型由路由成员提供'}<button className="text-button" onClick={onProviders}>管理供应商 <ChevronRight size={15}/></button></div>}</article>
   <article className="dashboard-card recent-sessions"><div className="dashboard-card-label"><MessageSquare size={22}/><h2>最近会话</h2><button className="text-button" onClick={onSessions}>查看全部 <ChevronRight size={15}/></button></div>{recent.length?<table><thead><tr><th>会话</th><th>项目</th><th>更新时间</th></tr></thead><tbody>{recent.map(s=><tr key={s.id}><td><button className="text-button" onClick={()=>onSession(s.id)} title={s.title}><FileText size={17}/><span>{s.title}</span></button></td><td title={s.cwd}>{s.cwd.split(/[\\/]/).pop()||s.cwd||'未关联'}</td><td>{new Date(s.updated*1000).toLocaleDateString('zh-CN',{month:'2-digit',day:'2-digit'})} {new Date(s.updated*1000).toLocaleTimeString('zh-CN',{hour:'2-digit',minute:'2-digit'})}</td></tr>)}</tbody></table>:<div className="dashboard-no-data">{loading?'正在读取会话…':scan?'这里还没有会话':'暂无数据'}</div>}</article>
   <article className="dashboard-card daily-summary"><div className="dashboard-card-label"><FileText size={22}/><h2>今日摘要</h2></div><div className="daily-total"><span>总计 Token</span><strong>{display(todayAvailable?sum(today!):null)}</strong></div><div className="daily-cache"><div><Database size={23}/><span>缓存输入<strong>{display(todayAvailable?today!.cached:null)}</strong></span></div><div><Percent size={23}/><span>缓存命中比例<strong>{loading?'…':todayAvailable&&today!.cached!==null&&today!.input>0?`${(today!.cached/today!.input*100).toFixed(1).replace(/\.0$/,'')}%`:'暂无数据'}</strong></span></div></div></article>
  </div>
 </section>;
}
