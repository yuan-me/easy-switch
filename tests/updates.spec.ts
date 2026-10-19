import {test,expect,type Page} from '@playwright/test';

async function desktop(page:Page){
 await page.addInitScript(()=>{
  const w=window as any,callbacks=new Map<number,Function>(),listeners=new Map<string,Function>();let id=0;
  w.installCalls=0;w.installFails=false;
  w.emitUpdate=(payload:unknown)=>listeners.get('update-status')?.({payload});
  Object.assign(w,{isTauri:true,__TAURI_EVENT_PLUGIN_INTERNALS__:{unregisterListener:()=>{}},__TAURI_INTERNALS__:{
   metadata:{currentWindow:{label:'main'},currentWebview:{label:'main'}},
   transformCallback:(callback:Function)=>{callbacks.set(++id,callback);return id},
   invoke:async(command:string,args:any)=>{
    if(command==='plugin:event|listen'){listeners.set(args.event,callbacks.get(args.handler)!);return args.handler}
    if(command==='bootstrap')return {providers:[],settings:{theme:'light',scrollPositions:{},automaticUpdates:true,automaticDownload:true},version:'1.3.0',migrationError:null};
    if(command==='usage_report')return {input:0,output:0,cached:null,buckets:[],models:[],sessions:[],availableModels:[],warnings:[],scannedFiles:0};
    if(command==='scan_sessions')return {sessions:[],warnings:[]};
    if(command==='interrupted_operations')return [];
    if(command==='check_update')return {state:'available',version:'1.3.1'};
    if(command==='download_update')return new Promise(resolve=>{w.finishDownload=()=>{const ready={state:'ready',version:'1.3.1'};w.emitUpdate(ready);resolve(ready)}});
    if(command==='install_update'){w.installCalls++;if(w.installFails)throw new Error('Codex 未完全退出；更新包已保留');return}
    return false;
   }
  }});
 });
 await page.goto('/');
 await page.getByRole('button',{name:'供应商',exact:true}).click();await expect(page.getByRole('heading',{name:'供应商',exact:true})).toBeVisible();
}

test('background download asks before installing, cancellation stays dismissed, failed installation can retry',async({page})=>{
 await desktop(page);
 await page.evaluate(()=>(window as any).emitUpdate({state:'ready',version:'1.3.1'}));
 const dialog=page.getByRole('dialog',{name:'关闭 Codex 并更新 Easy Switch'});
 await expect(dialog).toBeVisible();
 await expect(dialog).toContainText('确认后将正常关闭 Codex');
 expect(await page.evaluate(()=>(window as any).installCalls)).toBe(0);
 await dialog.getByRole('button',{name:'取消',exact:true}).click();
 await page.evaluate(()=>(window as any).emitUpdate({state:'ready',version:'1.3.1'}));
 await expect(dialog).toHaveCount(0);
 await page.getByRole('button',{name:'设置',exact:true}).click();
 await page.getByRole('button',{name:'安装更新',exact:true}).click();
 await expect(dialog).toBeVisible();
 expect(await page.evaluate(()=>(window as any).installCalls)).toBe(0);
 await page.evaluate(()=>{(window as any).installFails=true});
 await dialog.getByRole('button',{name:'关闭 Codex 并更新',exact:true}).click();
 await expect(page.getByRole('alert')).toContainText('更新包已保留');
 await expect(page.getByRole('button',{name:'安装更新',exact:true})).toBeEnabled();
 await expect(dialog).toHaveCount(0);
 await page.evaluate(()=>{(window as any).installFails=false});
 await page.getByRole('button',{name:'安装更新',exact:true}).click();
 await dialog.getByRole('button',{name:'关闭 Codex 并更新',exact:true}).click();
 await expect.poll(()=>page.evaluate(()=>(window as any).installCalls)).toBe(2);
});

test('manual download completion opens the same confirmation only when finished',async({page})=>{
 await desktop(page);
 await page.getByRole('button',{name:'设置',exact:true}).click();
 await page.getByRole('button',{name:'检查更新',exact:true}).click();
 await expect.poll(()=>page.evaluate(()=>typeof (window as any).finishDownload)).toBe('function');
 await expect(page.getByRole('dialog')).toHaveCount(0);
 expect(await page.evaluate(()=>(window as any).installCalls)).toBe(0);
 await page.evaluate(()=>(window as any).finishDownload());
 await expect(page.getByRole('dialog',{name:'关闭 Codex 并更新 Easy Switch'})).toBeVisible();
 expect(await page.evaluate(()=>(window as any).installCalls)).toBe(0);
});

test('ready update waits for an open editor and download errors never ask to install',async({page})=>{
 await desktop(page);
 await page.evaluate(()=>(window as any).emitUpdate({state:'error',message:'签名校验失败'}));
 await expect(page.getByRole('dialog')).toHaveCount(0);
 await page.getByRole('button',{name:'添加供应商',exact:true}).click();
 await expect(page.getByRole('dialog')).toBeVisible();
 await page.evaluate(()=>(window as any).emitUpdate({state:'ready',version:'1.3.1'}));
 await expect(page.getByRole('dialog')).toHaveCount(1);
 await expect(page.getByRole('dialog',{name:'关闭 Codex 并更新 Easy Switch'})).toHaveCount(0);
 await page.keyboard.press('Escape');
 await expect(page.getByRole('dialog',{name:'关闭 Codex 并更新 Easy Switch'})).toBeVisible();
 expect(await page.evaluate(()=>(window as any).installCalls)).toBe(0);
});
