import {setTheme} from './helpers';
import {test,expect,type Page} from '@playwright/test';
import {join} from 'node:path';
import {tmpdir} from 'node:os';

async function desktop(page:Page,mica:boolean|'error'=false){
 await page.addInitScript(mica=>{
  const w=window as any,callbacks=new Map<number,Function>(),listeners=new Map<string,Function>();let id=0;
  const settings={theme:'light',activeProviderId:null,scrollPositions:{}};
  w.emitProgress=(payload:unknown)=>listeners.get('operation-progress')?.({payload});
  w.emitResize=()=>listeners.get('tauri://resize')?.({payload:{width:1000,height:700}});
  w.calls=[];
  Object.assign(w,{isTauri:true,__TAURI_EVENT_PLUGIN_INTERNALS__:{unregisterListener:()=>{}},__TAURI_INTERNALS__:{
   metadata:{currentWindow:{label:'main'},currentWebview:{label:'main'}},
   transformCallback:(callback:Function)=>{callbacks.set(++id,callback);return id},
   invoke:async(command:string,args:any)=>{
    w.calls.push(command);
    if(command==='plugin:event|listen'){listeners.set(args.event,callbacks.get(args.handler)!);return args.handler}
    if(command==='bootstrap')return {providers:[{id:'official',name:'OpenAI 官方',mode:'Official',model:'',members:[],baseUrl:'',headers:{}}],settings,version:'1.1.2',migrationError:null};
    if(command==='window_appearance'){w.appearance=args;if(mica==='error')throw new Error('appearance unavailable');return mica}
    if(command==='save_settings'){Object.assign(settings,args.settings);return}
    if(command==='plugin:window|is_maximized')return false;
    if(command==='switch_provider')return new Promise((resolve,reject)=>{w.finish=()=>resolve('Codex 已启动');w.fail=()=>reject('模拟修复失败，原始文件未修改')});
    if(command==='cancel_operation'){w.fail();return}
    if(command==='usage_report')return {input:0,output:0,cached:null,buckets:[],models:[],sessions:[],availableModels:[],warnings:[],scannedFiles:0};
    if(command==='scan_sessions')return {sessions:[],warnings:[]};
    if(command==='interrupted_operations')return w.interrupted?[{id:'interrupted',description:'切换供应商',state:'Applying',fileCount:2,legacy:false}]:[];
    if(command==='restore_operation'){if(w.recoveryFails)throw new Error('检测到外部改动，拒绝覆盖');w.interrupted=false;return '中断操作已恢复'}
    return null;
   }
  }});
 },mica);
 await page.goto('/');
 await page.getByRole('button',{name:'供应商',exact:true}).click();await expect(page.getByRole('heading',{name:'供应商',exact:true})).toBeVisible();
}

test('only interrupted operations expose recovery, with external-edit errors preserved',async({page})=>{
 await page.addInitScript(()=>{Object.assign(window,{interrupted:true,recoveryFails:true})});
 await desktop(page);
 await expect(page.getByRole('navigation',{name:'主导航'}).getByRole('button')).toHaveCount(5);
 await expect(page.getByRole('button',{name:'备份恢复',exact:true})).toHaveCount(0);
 await expect(page.getByRole('alert')).toContainText('上次操作中断');
 await page.getByRole('button',{name:'恢复中断操作',exact:true}).click();
 await expect(page.getByRole('dialog')).toContainText('检测到外部修改时停止恢复');
 await page.getByRole('button',{name:'检查并恢复',exact:true}).click();
 await expect(page.getByRole('alert').filter({hasText:'拒绝覆盖'})).toBeVisible();
 await expect(page.getByRole('button',{name:'恢复中断操作',exact:true})).toBeEnabled();
 await page.evaluate(()=>{(window as any).recoveryFails=false});
 await page.getByRole('button',{name:'恢复中断操作',exact:true}).click();
 await page.getByRole('button',{name:'检查并恢复',exact:true}).click();
 await expect(page.getByRole('button',{name:'恢复中断操作',exact:true})).toHaveCount(0);
 await expect(page.getByRole('status')).toContainText('中断操作已恢复');
});

test('opaque fallback in both themes, Mica opt-in, and text selection',async({page})=>{
 const errors:string[]=[];page.on('pageerror',e=>errors.push(e.message));
 await desktop(page);
 await expect(page).toHaveTitle('Easy Switch');
 expect(page.url()).toBe('http://127.0.0.1:1420/');
 await expect(page.locator('html')).toHaveAttribute('data-backdrop','solid');
 await expect(page.locator('.app-shell')).toHaveCSS('background-color','rgb(243, 239, 244)');
 await expect(page.locator('h1')).toHaveCSS('user-select','none');
 await page.getByRole('heading',{name:'供应商',exact:true}).dblclick();
 expect(await page.evaluate(()=>getSelection()?.toString())).toBe('');
 const search=page.getByLabel('搜索供应商');await search.fill('test');await search.press('Control+A');
 expect(await search.evaluate(el=>(el as HTMLInputElement).selectionEnd!-(el as HTMLInputElement).selectionStart!)).toBe(4);
 await search.fill('');
 await setTheme(page,'深色');
 await expect(page.locator('.app-shell')).toHaveCSS('background-color','rgb(33, 30, 37)');
 await expect(page.locator('.operation-progress')).toHaveCount(0);
 await setTheme(page,'跟随系统');
 await page.emulateMedia({colorScheme:'dark'});
 await expect(page.locator('html')).toHaveAttribute('data-theme','dark');
 await expect.poll(()=>page.evaluate(()=>(window as any).appearance)).toMatchObject({system:true,dark:true});
 await page.emulateMedia({colorScheme:'light'});
 await expect(page.locator('html')).toHaveAttribute('data-theme','light');
 await expect.poll(()=>page.evaluate(()=>(window as any).appearance)).toMatchObject({system:true,dark:false});
 expect(errors).toEqual([]);
});

test('Mica is enabled only when the native operation succeeds',async({page})=>{
 await desktop(page,true);
 await expect(page.locator('html')).toHaveAttribute('data-backdrop','mica');
 await expect(page.locator('.app-shell')).toHaveCSS('background-color','rgba(243, 239, 244, 0.5)');
 await expect(page.locator('body')).toHaveCSS('background-color','rgba(0, 0, 0, 0)');
});

test('appearance errors retain the opaque fallback',async({page})=>{
 await desktop(page,'error');
 await expect(page.locator('html')).toHaveAttribute('data-backdrop','solid');
 await expect(page.locator('.app-shell')).toHaveCSS('background-color','rgb(243, 239, 244)');
 await expect.poll(()=>page.evaluate(()=>(window as any).calls.includes('window_ready'))).toBe(true);
});

for(const failed of [false,true])test(`startup reveals only after configuration and appearance settle (failure=${failed})`,async({page})=>{
 await page.addInitScript(failed=>{
  const w=window as any;w.startupCalls=[];w.revealed=[];
  Object.assign(w,{isTauri:true,__TAURI_EVENT_PLUGIN_INTERNALS__:{unregisterListener:()=>{}},__TAURI_INTERNALS__:{
   metadata:{currentWindow:{label:'main'},currentWebview:{label:'main'}},transformCallback:()=>1,
   invoke:async(command:string)=>{
    w.startupCalls.push(command);
    if(command==='usage_report')return {input:0,output:0,cached:null,buckets:[],models:[],sessions:[],availableModels:[],warnings:[],scannedFiles:0};
    if(command==='scan_sessions')return {sessions:[],warnings:[]};
    if(command==='interrupted_operations')return [];
    if(command==='bootstrap')return new Promise((resolve,reject)=>{w.finishBootstrap=()=>failed?reject(new Error('配置读取失败')):resolve({providers:[],settings:{theme:'dark',scrollPositions:{}},version:'test',migrationError:null})});
    if(command==='window_appearance')return new Promise((resolve,reject)=>{w.finishAppearance=()=>failed?reject(new Error('Mica unavailable')):resolve(true)});
    if(command==='window_ready')w.revealed.push({theme:document.documentElement.dataset.theme,backdrop:document.documentElement.dataset.backdrop,loading:!!document.querySelector('.loading')});
    return false;
   }
  }});
 },failed);
 await page.goto('/');await expect(page.getByText('读取本地配置…')).toBeVisible();
 expect(await page.evaluate(()=>(window as any).startupCalls.includes('window_appearance'))).toBe(false);
 expect(await page.evaluate(()=>(window as any).revealed)).toEqual([]);
 await page.evaluate(()=>(window as any).finishBootstrap());
 await expect.poll(()=>page.evaluate(()=>(window as any).startupCalls.filter((c:string)=>c==='window_appearance').length)).toBe(1);
 expect(await page.evaluate(()=>(window as any).revealed)).toEqual([]);
 await page.evaluate(()=>(window as any).finishAppearance());
 await expect.poll(()=>page.evaluate(()=>(window as any).revealed)).toEqual([{theme:failed?'light':'dark',backdrop:failed?'solid':'mica',loading:false}]);
 if(failed)await expect(page.getByText('无法读取本地配置')).toBeVisible();
});

test('editor avoids full-window blur and translucent animation; resize queries are coalesced',async({page})=>{
 await page.clock.install();await desktop(page);
 const calls=()=>page.evaluate(()=>(window as any).calls.filter((c:string)=>c==='plugin:window|is_maximized').length);
 const before=await calls();
 await page.evaluate(()=>{for(let i=0;i<100;i++)(window as any).emitResize()});
 expect(await calls()).toBe(before);
 await page.clock.runFor(150);expect(await calls()).toBe(before+1);
 await page.getByRole('button',{name:'添加供应商',exact:true}).click();
 const dialog=page.getByRole('dialog');
 expect(await dialog.evaluate(el=>getComputedStyle(el,'::backdrop').backdropFilter)).toBe('none');
 await expect(dialog).toHaveCSS('opacity','1');
 await expect(dialog).toHaveCSS('will-change','transform');
 await page.emulateMedia({reducedMotion:'reduce'});
 await expect(dialog).toHaveCSS('animation-name','none');
 await expect(dialog).toHaveCSS('will-change','auto');
});

test('confirmation and progress remain opaque over Mica in both themes',async({page})=>{
 const errors:string[]=[];page.on('pageerror',e=>errors.push(e.message));
 await desktop(page,true);
 for(const [theme,color] of [['浅色','rgb(255, 252, 255)'],['深色','rgb(44, 39, 50)']]){
  await setTheme(page,theme);
  await page.getByRole('button',{name:'切换',exact:true}).click();
  const dialog=page.getByRole('dialog');
  await expect(dialog).toBeVisible();
  await expect(dialog).toHaveCSS('background-color',color);
  await expect(dialog.locator('.modal-footer')).toHaveCSS('background-color',color);
  await page.screenshot({animations:'disabled',path:join(tmpdir(),`easy-switch-confirm-${theme==='浅色'?'light':'dark'}.png`)});
  await dialog.getByRole('button',{name:'切换并重启',exact:true}).click();
  await expect(page.locator('.operation-progress')).toBeVisible();
  await expect(page.locator('.operation-progress')).toHaveCSS('background-color',color);
  await page.evaluate(()=>(window as any).finish());
  await expect(page.locator('.operation-progress')).toHaveCount(0);
 }
 expect(errors).toEqual([]);
});

test('switch progress, elapsed time, safe commit, completion and cancellation',async({page})=>{
 const errors:string[]=[];page.on('pageerror',e=>errors.push(e.message));
 await page.clock.install();await desktop(page);
 await page.emulateMedia({colorScheme:'light'});
 await setTheme(page,'跟随系统');
 const begin=async()=>{await page.getByRole('button',{name:'切换',exact:true}).click();await page.getByRole('button',{name:'切换并重启',exact:true}).click();await expect(page.locator('.operation-progress')).toBeVisible()};
 const emit=async(stage:string,detail:string,completed=0,total:number|null=null,cancellable=true)=>page.evaluate(p=>(window as any).emitProgress(p),{stage,detail,completed,total,cancellable});
 await begin();
 await emit('扫描会话','已发现 77 个会话文件',77);
 await expect(page.locator('.progress-detail')).toHaveText('已发现 77 个会话文件');
 await page.clock.runFor(100);
 await expect(page.locator('progress')).not.toHaveAttribute('value');
 await page.clock.fastForward(16000);
 await expect(page.locator('.progress-meta')).toContainText('已用 16 秒');
 await expect(page.locator('.progress-meta')).toContainText('最近进展在');
 await emit('修复会话','备份并修复分页历史文件',38,77);
 await expect(page.locator('.operation-progress')).toContainText('当前子任务 49%');
 await expect(page.locator('progress')).toHaveAttribute('value','38');
 await expect(page.locator('progress')).toHaveAttribute('max','77');
 await expect(page.getByRole('button',{name:'备份恢复',exact:true})).toHaveCount(0);
 await page.getByRole('button',{name:'设置',exact:true}).click();
 await page.clock.runFor(300);
 await expect(page.locator('.operation-progress')).toHaveCSS('background-color','rgb(255, 252, 255)');
 await expect(page.locator('.operation-progress')).not.toHaveCSS('box-shadow','none');
 await page.screenshot({animations:'disabled',path:join(tmpdir(),'easy-switch-progress-light.png')});
 await page.setViewportSize({width:940,height:620});
 await expect(page.locator('.operation-progress')).toBeInViewport();
 const panel=await page.locator('.operation-progress').boundingBox(),main=await page.locator('main').boundingBox();
 expect(panel!.x).toBeGreaterThanOrEqual(main!.x);
 expect(panel!.x+panel!.width).toBeLessThanOrEqual(main!.x+main!.width);
 expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
 await page.screenshot({animations:'disabled',path:join(tmpdir(),'easy-switch-progress-minimum.png')});
 await page.emulateMedia({colorScheme:'dark'});
 await expect(page.locator('.operation-progress')).toHaveCSS('background-color','rgb(44, 39, 50)');
 await page.getByRole('button',{name:'取消',exact:true}).focus();
 await page.keyboard.press('Shift+Tab');
 await page.keyboard.press('Tab');
 await expect(page.getByRole('button',{name:'取消',exact:true})).toHaveCSS('outline-style','solid');
 await page.screenshot({animations:'disabled',path:join(tmpdir(),'easy-switch-progress-dark-minimum.png')});
 await page.setViewportSize({width:1280,height:900});
 await page.screenshot({animations:'disabled',path:join(tmpdir(),'easy-switch-progress-dark.png')});
 await emit('提交与校验','写入并校验文件（此阶段不可取消）',150,154,false);
 await expect(page.getByRole('button',{name:'请稍候',exact:true})).toBeDisabled();
 await emit('重新启动','等待代理与 Codex 窗口启动',0,null,false);
 await expect(page.locator('progress')).not.toHaveAttribute('value');
 await page.emulateMedia({reducedMotion:'reduce'});
 await expect(page.locator('.operation-progress .spin')).toHaveCSS('animation-name','none');
 await page.screenshot({animations:'disabled',path:join(tmpdir(),'easy-switch-progress-waiting.png')});
 await page.evaluate(()=>(window as any).finish());
 await expect(page.locator('.operation-progress')).toHaveCount(0);
 await expect(page.locator('.notice')).toContainText('切换成功');
 await page.getByRole('button',{name:'供应商',exact:true}).click();
 await begin();await emit('修复会话','检查会话关联',0,0);
 await expect(page.locator('.operation-progress')).not.toContainText('NaN');
 await page.getByRole('button',{name:'取消',exact:true}).click();
 await expect(page.locator('.operation-progress')).toHaveCount(0);
 await expect(page.getByRole('alert')).toContainText('原始文件未修改');
 expect(errors).toEqual([]);
});
