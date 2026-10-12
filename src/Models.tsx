import {useState} from 'react';
import {Search} from 'lucide-react';

export function ModelPicker({available,excluded,defaultModel,added=[],disabled=false,onChange}:{available:string[];excluded:string[];defaultModel:string;added?:string[];disabled?:boolean;onChange:(excluded:string[])=>void}){
 const [query,setQuery]=useState('');
 const names=Array.from(new Set([...available,...(defaultModel?[defaultModel]:[])])).sort();
 const shown=names.filter(id=>id.toLowerCase().includes(query.toLowerCase()));
 const selected=names.filter(id=>id===defaultModel||!excluded.includes(id)).length;
 const allSelected=selected===names.length;
 return <div className="model-picker"><div className="model-picker-heading"><strong>加入 Codex 的模型</strong><div className="model-picker-actions"><span>{selected} / {names.length}</span><button type="button" disabled={disabled||!names.some(id=>id!==defaultModel)} title={allSelected?'取消全部可选模型，保留默认模型':'选中全部模型，不受搜索筛选影响'} onClick={()=>onChange([...excluded.filter(id=>!names.includes(id)),...(allSelected?names.filter(id=>id!==defaultModel):[])])}>{allSelected?'取消全选':'全选'}</button></div></div><label className="search"><Search size={15}/><input aria-label="搜索模型" placeholder="搜索模型 ID" value={query} onChange={e=>setQuery(e.target.value)}/></label><div className="model-choices">{shown.map(id=><label className="model-choice" key={id}><input type="checkbox" aria-label={'加入 '+id} disabled={disabled||id===defaultModel} checked={id===defaultModel||!excluded.includes(id)} onChange={e=>onChange(e.target.checked?excluded.filter(x=>x!==id):[...excluded.filter(x=>x!==id),id])}/><span title={id}>{id}</span>{id===defaultModel?<small>默认</small>:added.includes(id)?<small className="new-model-label">NEW</small>:null}</label>)}{!shown.length&&<p className="model-empty">没有匹配的模型</p>}</div><small className="model-picker-help">取消勾选即可移除；自动刷新会保留你的选择。更换默认模型后可移除原默认模型。</small></div>
}
