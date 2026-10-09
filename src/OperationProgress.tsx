import {useEffect,useState} from 'react';
import {Loader2} from 'lucide-react';
import {call} from './api';
import type {Progress} from './types';

export function OperationProgress({progress,onError}:{progress:Progress|null;onError:(text:string)=>void}) {
 const [started]=useState(Date.now),[now,setNow]=useState(Date.now),[updated,setUpdated]=useState(Date.now),[cancelling,setCancelling]=useState(false);
 useEffect(()=>{const timer=setInterval(()=>setNow(Date.now()),1000);return()=>clearInterval(timer)},[]);
 useEffect(()=>{setUpdated(Date.now())},[progress]);
 const percent=progress?.total?Math.min(100,Math.floor(progress.completed/progress.total*100)):undefined;
 const elapsed=Math.max(0,Math.floor((now-started)/1000)),quiet=Math.max(0,Math.floor((now-updated)/1000));
 async function cancel(){setCancelling(true);try{await call('cancel_operation')}catch(e){setCancelling(false);onError(String(e))}}
 return <div className="operation-progress">
  <Loader2 className="spin" size={18}/>
  <div className="progress-content">
   <div className="progress-heading" role="status"><strong>{progress?.stage??'正在处理'}</strong><span>{percent!==undefined?`当前子任务 ${percent}%`:'处理中'}</span></div>
   {progress?.detail&&<span className="progress-detail">{progress.detail}</span>}
   <progress aria-label="当前子任务进度" max={progress?.total||1} value={percent===undefined?undefined:progress?.completed}/>
   <span className="progress-meta">{percent!==undefined&&`${progress?.completed} / ${progress?.total} 项 · `}已用 {elapsed} 秒{quiet>=15&&` · 最近进展在 ${quiet} 秒前，等待当前任务完成`}</span>
  </div>
  <button onClick={()=>void cancel()} disabled={cancelling||progress?.cancellable===false}>{cancelling?'正在取消…':progress?.cancellable===false?'请稍候':'取消'}</button>
 </div>
}
