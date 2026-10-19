import {useEffect,useRef,useState} from 'react';
import {call} from './api';
import type {Settings} from './types';

export function useSettingsAutosave(settings:Settings|null,onSaved:(patch:Partial<Settings>)=>void){
 const [pending,setPending]=useState<Partial<Settings>>({});
 const [saving,setSaving]=useState(false),[error,setError]=useState('');
 const base=useRef(settings),draft=useRef(pending),revision=useRef(0),inFlight=useRef(false);
 const flushRef=useRef<()=>Promise<void>>(async()=>{});
 base.current=settings;
 function change(patch:Partial<Settings>){
  draft.current={...draft.current,...patch};revision.current++;
  setPending(draft.current);setError('');
 }
 async function flush(){
  if(inFlight.current||!base.current||!Object.keys(draft.current).length)return;
  const patch={...draft.current},started=revision.current;
  inFlight.current=true;setSaving(true);setError('');
  try{
   await call('save_settings',{settings:{...base.current,...patch}});
   base.current={...base.current,...patch};onSaved(patch);
   // A completed write must not discard edits made while it was in flight.
   draft.current=Object.fromEntries(Object.entries(draft.current).filter(([key,value])=>!Object.hasOwn(patch,key)||patch[key as keyof Settings]!==value));
   setPending(draft.current);
  }catch(e){setError(String(e))}
  finally{
   inFlight.current=false;setSaving(false);
   if(revision.current!==started)void flushRef.current();
  }
 }
 flushRef.current=flush;
 useEffect(()=>{
  if(error||!Object.keys(pending).length)return;
  const timer=setTimeout(()=>void flushRef.current(),400);
  return()=>clearTimeout(timer);
 },[pending,error]);
 return {settings:settings?{...settings,...pending}:null,change,retry:flush,error,
  status:saving||Object.keys(pending).length?'正在自动保存…':'已自动保存'};
}
