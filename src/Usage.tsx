import {useEffect,useMemo,useState} from 'react';
import {ChevronLeft,ChevronRight,RefreshCw,Loader2} from 'lucide-react';
import {call} from './api';
import type {UsageReport,UsageShare} from './types';

type Period='day'|'week'|'month';
const number=new Intl.NumberFormat('zh-CN');
const compact=new Intl.NumberFormat('en',{notation:'compact',maximumFractionDigits:2});
const total=(v:{input:number;output:number})=>v.input+v.output;
function localDate(d:Date){return [d.getFullYear(),String(d.getMonth()+1).padStart(2,'0'),String(d.getDate()).padStart(2,'0')].join('-')}
export function periodRange(date:string,period:Period){
 const start=new Date(date+'T00:00:00');
 if(period==='week')start.setDate(start.getDate()-(start.getDay()+6)%7);
 if(period==='month')start.setDate(1);
 const end=new Date(start);
 if(period==='month')end.setMonth(end.getMonth()+1);else end.setDate(end.getDate()+(period==='week'?7:1));
 const starts:number[]=[];
 for(let d=new Date(start);d<end;){
  starts.push(d.getTime()/1000);
  if(period==='day')d=new Date(d.getTime()+3600000);else d.setDate(d.getDate()+1);
 }
 return {start,end,bucketStarts:starts};
}
export default function Usage({onSession}:{onSession:(id:string)=>void}){
 const [period,setPeriod]=useState<Period>('week'),[date,setDate]=useState(()=>localDate(new Date())),[model,setModel]=useState(''),[revision,setRevision]=useState(0);
 const [report,setReport]=useState<UsageReport|null>(null),[loading,setLoading]=useState(true),[error,setError]=useState(''),[allSessions,setAllSessions]=useState(false);
 const range=useMemo(()=>periodRange(date,period),[date,period]);
 useEffect(()=>{
  let active=true;setLoading(true);setError('');
  call<UsageReport>('usage_report',{query:{bucketStarts:range.bucketStarts,end:range.end.getTime()/1000,model:model||null}})
   .then(value=>{if(active)setReport(value)}).catch(e=>{if(active){setReport(null);setError(String(e))}}).finally(()=>{if(active)setLoading(false)});
  return()=>{active=false};
 },[range,model,revision]);
 useEffect(()=>{const id=setInterval(()=>{if(!document.hidden)setRevision(v=>v+1)},60000);return()=>clearInterval(id)},[]);
 function move(direction:number){const next=new Date(range.start);if(period==='month')next.setMonth(next.getMonth()+direction);else next.setDate(next.getDate()+direction*(period==='week'?7:1));setDate(localDate(next))}
 const endLabel=new Date(range.end.getTime()-1).toLocaleDateString('zh-CN',{month:'2-digit',day:'2-digit'});
 const peak=Math.max(1,...(report?.buckets.map(total)??[])),power=10**Math.floor(Math.log10(peak)),ceiling=Math.ceil(peak/power)*power;
 const sum=report?total(report):0;
 const ratio=report&&report.cached!==null&&report.input>0?number.format(Math.round(report.cached/report.input*1000)/10)+'%':'—';
 function shareRows(rows:UsageShare[],session=false){return rows.map(row=><div className="usage-share" key={row.id}>
  {session?<button className="usage-session-link" title={row.name} onClick={()=>onSession(row.id)}>{row.name}</button>:<span title={row.name}>{row.name}</span>}
  <span className="usage-track" aria-hidden="true"><i style={{width:(sum?total(row)/sum*100:0)+'%'}}/></span>
  <strong title={number.format(total(row))}>{compact.format(total(row))}</strong><small>{sum?(total(row)/sum*100).toFixed(1):'0'}%</small>
 </div>)}
 return <section className="page usage-page" aria-busy={loading}>
  <div className="page-heading"><h1>Token 统计</h1><button onClick={()=>setRevision(v=>v+1)} disabled={loading} aria-label="刷新统计">{loading?<Loader2 className="spin" size={16}/>:<RefreshCw size={16}/>}刷新</button></div>
  <div className="toolbar usage-toolbar">
   <div className="segmented" aria-label="统计周期">{([['day','日'],['week','周'],['month','月']] as const).map(([value,label])=><button key={value} aria-pressed={period===value} onClick={()=>setPeriod(value)}>{label}</button>)}</div>
   <div className="usage-date"><button aria-label="上一周期" onClick={()=>move(-1)}><ChevronLeft size={16}/></button><label title="按本机时区统计"><input aria-label="统计日期" type="date" value={localDate(range.start)} max={localDate(new Date())} onChange={e=>{if(e.target.value)setDate(e.target.value)}}/>{period!=='day'&&<span>— {endLabel}</span>}</label><button aria-label="下一周期" disabled={range.end>new Date()} onClick={()=>move(1)}><ChevronRight size={16}/></button></div>
   <select aria-label="统计模型" value={model} onChange={e=>setModel(e.target.value)}><option value="">全部模型</option>{[...new Set([...(report?.availableModels??[]),...(model?[model]:[])])].map(m=><option key={m}>{m}</option>)}</select>
  </div>
  {error&&<div className="warning-box" role="alert">{error}<button onClick={()=>setRevision(v=>v+1)}>重试</button></div>}
  {report?.warnings.length? <details className="warning-box"><summary>部分记录未纳入统计（{report.warnings.length} 项）</summary>{report.warnings.map((w,i)=><p key={i}>{w}</p>)}</details>:null}
  {!report&&loading?<div className="loading"><Loader2 className="spin"/>正在汇总本地 Token 记录…</div>:report&&<>
   <div className="usage-metrics">
    {([['总 Token',compact.format(sum)],['输入 Token',compact.format(report.input)],['输出 Token',compact.format(report.output)],['缓存命中比例',ratio]] as const).map(([name,value])=><div key={name}><span>{name}</span><strong data-testid={name==='总 Token'?'usage-total':undefined}>{value}</strong>{name==='缓存命中比例'&&<small>缓存输入 {report.cached===null?'—':compact.format(report.cached)}</small>}</div>)}
   </div>
   <div className="usage-panel usage-chart">
    <div className="usage-panel-heading"><div><h2>{period==='day'?'每小时':'每日'} Token 用量</h2><p>{range.start.toLocaleDateString('zh-CN')} — {endLabel} · {period==='day'?'按小时':'按天'}汇总</p></div><div className="usage-legend"><span><i/>输入</span><span><i/>输出</span></div></div>
    {sum===0&&<p className="usage-empty">所选范围暂无 Token 用量记录</p>}
    <div className="usage-plot">
     <div className="usage-grid" aria-hidden="true">{[1,.75,.5,.25,0].map(n=><div key={n} style={{top:(1-n)*100+'%'}}><span>{compact.format(ceiling*n)}</span></div>)}</div>
     <div className={'usage-columns '+(report.buckets.length>10?'dense':'')}>{report.buckets.map((bucket,i)=>{
      const d=new Date(bucket.start*1000),future=d>new Date(),value=total(bucket),label=period==='day'?String(d.getHours()).padStart(2,'0')+':00':period==='week'?['周日','周一','周二','周三','周四','周五','周六'][d.getDay()]:String(d.getDate());
      const description=d.toLocaleString('zh-CN')+'，输入 '+number.format(bucket.input)+'，输出 '+number.format(bucket.output);
      return <div className="usage-column" key={bucket.start} tabIndex={0} role="img" aria-label={description} title={description}>
       <div className="usage-stack" style={{height:Math.max(0,value/ceiling*100)+'%'}}>{(report.buckets.length<=7||value===peak)&&<small>{future?'—':compact.format(value)}</small>}<i className="usage-output" style={{height:(value?bucket.output/value*100:0)+'%'}}/><i className="usage-input" style={{height:(value?bucket.input/value*100:0)+'%'}}/></div>
       <span className="usage-tick">{report.buckets.length<=10||i%Math.ceil(report.buckets.length/10)===0?label:''}{period==='week'&&<small>{String(d.getMonth()+1).padStart(2,'0')}.{String(d.getDate()).padStart(2,'0')}</small>}</span>
      </div>})}</div>
    </div>
   </div>
   <div className="usage-breakdown">
    <div className="usage-panel"><h2>模型分布</h2><div className="usage-share-list">{shareRows(report.models)}{!report.models.length&&<p className="muted">暂无记录</p>}</div></div>
    <div className="usage-panel"><div className="usage-panel-heading"><h2>会话消耗排行</h2>{report.sessions.length>5&&<button className="text-button" onClick={()=>setAllSessions(v=>!v)}>{allSessions?'收起':'显示全部'}</button>}</div><div className="usage-share-list">{shareRows(allSessions?report.sessions:report.sessions.slice(0,5),true)}{!report.sessions.length&&<p className="muted">暂无记录</p>}</div></div>
   </div>
  </>}
 </section>
}
