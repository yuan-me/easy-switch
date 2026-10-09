import {useEffect,useState} from 'react';
import {Minus,Square,Copy,X} from 'lucide-react';
import {getCurrentWindow} from '@tauri-apps/api/window';
import {call,preview} from './api';
import {Brand} from './components';

export function Titlebar({theme,onError}:{theme:'light'|'dark'|'system';onError:(text:string)=>void}) {
 const [maximized,setMaximized]=useState(false);
 useEffect(()=>{
  if(preview)return;
  const win=getCurrentWindow();let disposed=false;
  const update=()=>win.isMaximized().then(value=>{if(!disposed)setMaximized(value)});
  update().catch(()=>{});
  const listener=win.onResized(()=>{update().catch(()=>{})});
  return()=>{disposed=true;listener.then(unlisten=>unlisten()).catch(()=>{})};
 },[]);
 useEffect(()=>{
  if(preview)return;
  const media=matchMedia('(prefers-color-scheme: dark)');let disposed=false;
  const apply=async()=>{
   try{const mica=await call<boolean>('window_appearance',{dark:theme==='dark'||(theme==='system'&&media.matches),system:theme==='system'});if(!disposed)document.documentElement.dataset.backdrop=mica?'mica':'solid';}catch(e){if(!disposed){document.documentElement.dataset.backdrop='solid';onError(String(e))}}
  };
  void apply();media.addEventListener('change',apply);return()=>{disposed=true;media.removeEventListener('change',apply)};
 // onError is an event sink, not a theme dependency.
 // eslint-disable-next-line react-hooks/exhaustive-deps
 },[theme]);
 async function action(kind:'minimize'|'maximize'|'close'){
  if(preview){if(kind==='maximize')setMaximized(v=>!v);return;}
  try{const win=getCurrentWindow();if(kind==='maximize')await win.toggleMaximize();else await win[kind]()}catch(e){onError(String(e))}
 }
 return <header className="window-titlebar">
  <div className="titlebar-drag" data-tauri-drag-region>
   <Brand small/><strong>Easy Switch</strong>{preview&&<span className="demo-label">演示数据</span>}
  </div>
  <div className="window-controls">
   <button aria-label="最小化" title="最小化" onClick={()=>void action('minimize')}><Minus size={16}/></button>
   <button aria-label={maximized?'还原窗口':'最大化'} title={maximized?'还原窗口':'最大化'} onClick={()=>void action('maximize')}>{maximized?<Copy size={14}/>:<Square size={14}/>}</button>
   <button className="window-close" aria-label="关闭窗口" title="关闭窗口" onClick={()=>void action('close')}><X size={18}/></button>
  </div>
 </header>
}
