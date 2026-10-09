import {test,expect,type Page} from '@playwright/test';
import {join} from 'node:path';
import {tmpdir} from 'node:os';

async function desktop(page:Page,mica:boolean|'error'=false){
 await page.addInitScript(mica=>{
  const w=window as any,callbacks=new Map<number,Function>(),listeners=new Map<string,Function>();let id=0;
  const settings={theme:'light',activeProviderId:null,scrollPositions:{}};
  w.emitProgress=(payload:unknown)=>listeners.get('operation-progress')?.({payload});
  w.calls=[];
  Object.assign(w,{isTauri:true,__TAURI_EVENT_PLUGIN_INTERNALS__:{unregisterListener:()=>{}},__TAURI_INTERNALS__:{
   metadata:{currentWindow:{label:'main'},currentWebview:{label:'main'}},
   transformCallback:(callback:Function)=>{callbacks.set(++id,callback);return id},
   invoke:async(command:string,args:any)=>{
    w.calls.push(command);
    if(command==='plugin:event|listen'){listeners.set(args.event,callbacks.get(args.handler)!);return args.handler}
    if(command==='bootstrap')return {providers:[{id:'official',name:'OpenAI 官方',mode:'Official',model:'',members:[],baseUrl:'',headers:{}}],settings,version:'1.1.1',migrationError:null};
    if(command==='window_appearance'){w.appearance=args;if(mica==='error')throw new Error('appearance unavailable');return mica}
    if(command==='save_settings'){Object.assign(settings,args.settings);return}
    if(command==='plugin:window|is_maximized')return false;
    if(command==='switch_provider')return new Promise((resolve,reject)=>{w.finish=()=>resolve('Codex 已启动');w.fail=()=>reject('模拟修复失败，原始文件未修改')});
    if(command==='cancel_operation'){w.fail();return}
    if(command==='list_operations')return [];
    return null;
   }
  }});
 },mica);
 await page.goto('/');
 await expect(page.getByRole('heading',{name:'供应商',exact:true})).toBeVisible();
}

test('opaque fallback in both themes, Mica opt-in, and text selection',async({page})=>{
 const errors:string[]=[];page.on('pageerror',e=>errors.push(e.message));
 await desktop(page);
 await expect(page).toHaveTitle('Easy Switch');
 expect(page.url()).toBe('http://127.0.0.1:1420/');
 await expect(page.locator('html')).toHaveAttribute('data-backdrop','solid');
 await expect(page.locator('.app-shell')).toHaveCSS('background-color','rgb(242, 246, 252)');
 await expect(page.locator('h1')).toHaveCSS('user-select','none');
 await page.getByRole('heading',{name:'供应商',exact:true}).dblclick();
 expect(await page.evaluate(()=>getSelection()?.toString())).toBe('');
 const search=page.getByLabel('搜索供应商');await search.fill('test');await search.press('Control+A');
 expect(await search.evaluate(el=>(el as HTMLInputElement).selectionEnd!-(el as HTMLInputElement).selectionStart!)).toBe(4);
 await search.fill('');
 await page.getByLabel('深色',{exact:true}).click();
 await expect(page.locator('.app-shell')).toHaveCSS('background-color','rgb(20, 28, 41)');
 await expect(page.locator('.operation-progress')).toHaveCount(0);
 await page.getByLabel('跟随系统',{exact:true}).click();
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
 await expect(page.locator('.app-shell')).toHaveCSS('background-color','rgba(242, 246, 252, 0.5)');
 await expect(page.locator('body')).toHaveCSS('background-color','rgba(0, 0, 0, 0)');
});

test('appearance errors retain the opaque fallback',async({page})=>{
 await desktop(page,'error');
 await expect(page.locator('html')).toHaveAttribute('data-backdrop','solid');
 await expect(page.locator('.app-shell')).toHaveCSS('background-color','rgb(242, 246, 252)');
});

test('switch progress, elapsed time, safe commit, completion and cancellation',async({page})=>{
 const errors:string[]=[];page.on('pageerror',e=>errors.push(e.message));
 await page.clock.install();await desktop(page);
 const begin=async()=>{await page.getByRole('button',{name:'切换',exact:true}).click();await page.getByRole('button',{name:'切换并重启',exact:true}).click();await expect(page.locator('.operation-progress')).toBeVisible()};
 const emit=async(stage:string,detail:string,completed=0,total:number|null=null,cancellable=true)=>page.evaluate(p=>(window as any).emitProgress(p),{stage,detail,completed,total,cancellable});
 await begin();
 await emit('扫描会话','已发现 77 个会话文件',77);
 await expect(page.locator('progress')).not.toHaveAttribute('value');
 await page.clock.fastForward(16000);
 await expect(page.locator('.progress-meta')).toContainText('已用 16 秒');
 await expect(page.locator('.progress-meta')).toContainText('最近进展在');
 await emit('修复会话','备份并修复分页历史文件',38,77);
 await expect(page.locator('.operation-progress')).toContainText('当前子任务 49%');
 await expect(page.locator('progress')).toHaveAttribute('value','38');
 await expect(page.locator('progress')).toHaveAttribute('max','77');
 await page.getByRole('button',{name:'备份恢复',exact:true}).click();
 await page.clock.runFor(300);
 await page.screenshot({animations:'disabled',path:join(tmpdir(),'easy-switch-progress-light.png')});
 await page.setViewportSize({width:940,height:620});
 await expect(page.locator('.operation-progress')).toBeInViewport();
 expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
 await page.screenshot({animations:'disabled',path:join(tmpdir(),'easy-switch-progress-minimum.png')});
 await emit('提交与校验','写入并校验文件（此阶段不可取消）',150,154,false);
 await expect(page.getByRole('button',{name:'请稍候',exact:true})).toBeDisabled();
 await emit('重新启动','等待代理与 Codex 窗口启动',0,null,false);
 await expect(page.locator('progress')).not.toHaveAttribute('value');
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
