import {createElement,useLayoutEffect,useRef,type ReactNode,type SelectHTMLAttributes} from 'react';
import {X,PanelLeft} from 'lucide-react';
export function IconButton({label,children,onClick,disabled}:{label:string;children:ReactNode;onClick:()=>void;disabled?:boolean}){return <button type="button" className="icon-button" aria-label={label} title={label} onClick={onClick} disabled={disabled}>{children}</button>}
export function Select({children,...props}:SelectHTMLAttributes<HTMLSelectElement>){
 return <select {...props}><button type="button">{createElement('selectedcontent')}</button>{children}</select>
}
export function Brand({small=false}:{small?:boolean}){return <span className={'brand-mark '+(small?'small':'')} style={{background:'transparent'}}><img src={new URL('../src-tauri/icons/128x128.png',import.meta.url).href} width={small?32:38} height={small?32:38} alt="" aria-hidden="true"/></span>}
export function Modal({title,children,onClose,wide=false}:{title:string;children:ReactNode;onClose:()=>void;wide?:boolean}){
 const ref=useRef<HTMLDialogElement>(null);
 useLayoutEffect(()=>{
  const dialog=ref.current!;dialog.showModal();
  // React autoFocus runs before a newly mounted dialog is visible.
  dialog.querySelector<HTMLElement>('[data-autofocus]')?.focus({preventScroll:true});
  return()=>dialog.close();
 },[]);
 return <dialog ref={ref} className={wide?'modal provider-editor':'modal'} onCancel={e=>{e.preventDefault();onClose()}} aria-label={title} onClick={e=>{if(e.target===e.currentTarget)onClose()}}><div className="modal-head"><h2>{title}</h2><IconButton label="关闭" onClick={onClose}><X size={19}/></IconButton></div>{children}</dialog>
}
export function Empty({title,children}:{title:string;children?:ReactNode}){return <div className="empty"><span className="empty-icon"><PanelLeft size={28}/></span><h3>{title}</h3>{children}</div>}
export type Confirm={title:string;text:string;action:string;run:()=>Promise<void>;danger?:boolean};
export type Run=(fn:()=>Promise<void>)=>Promise<void>;
export type Notify=(text:string,error?:boolean)=>void;
