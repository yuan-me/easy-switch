import {test,expect,type Page} from '@playwright/test';

async function setupFast(page:Page,{active=true,failSave=false}={}){
 await page.addInitScript(({active,failSave})=>{
  const provider={id:'api',name:'Sub2API',mode:'Api',protocol:'Responses',baseUrl:'https://api.example.com/v1',model:'codex-main',members:[],headers:{},strategy:'Failover',hasKey:true,imageCompatibility:false,nativeFastUnlock:false,fastModels:[]};
  const state={providers:[provider,{...provider,id:'other',name:'另一供应商'}],settings:{theme:'light',codexHome:'C:\\Demo',desktopExecutable:'',sqliteHome:null,desktopAppId:null,runtimePort:47831,scrollPositions:{},activeProviderId:active?'api':null,automaticUpdates:false,automaticDownload:false},modelCatalogs:{api:{available:['codex-main','codex-fast'],excluded:[],added:[],removed:[],checkedAt:1,error:null}},version:'1.3.0',migrationError:null};
  const controls={failSave,restarts:[] as boolean[],providers:state.providers};Object.assign(window,{fastTest:controls});
  Object.assign(window,{isTauri:true,__TAURI_EVENT_PLUGIN_INTERNALS__:{unregisterListener:()=>{}},__TAURI_INTERNALS__:{metadata:{currentWindow:{label:'main'},currentWebview:{label:'main'}},transformCallback:()=>1,invoke:async(command:string,args:any)=>{
   if(command==='bootstrap')return structuredClone(state);
   if(command==='save_settings'){Object.assign(state.settings,args.settings);return}
   if(command==='switch_provider'){controls.restarts.push(state.providers.find(p=>p.id===args.id)!.nativeFastUnlock);return 'Codex 已启动'}
   if(command==='save_provider'){await new Promise(r=>setTimeout(r,100));if(controls.failSave)throw new Error('模拟保存失败');const index=state.providers.findIndex(p=>p.id===args.provider.id);if(index<0)state.providers.push(args.provider);else state.providers[index]=args.provider;return}
   if(command==='interrupted_operations')return [];
   if(command==='scan_sessions')return {sessions:[],warnings:[]};
   if(command==='usage_report')return {input:0,output:0,cached:0,buckets:[],models:[],sessions:[],warnings:[],scannedFiles:0};
   return false;
  }}});
 },{active,failSave});
}
async function edit(page:Page,name='Sub2API'){
 await page.getByRole('button',{name:'供应商',exact:true}).click();
 await page.locator('.provider-card').filter({has:page.getByRole('heading',{name})}).getByRole('button',{name:'编辑',exact:true}).click();
 const dialog=page.getByRole('dialog',{name:'编辑供应商'});
 await dialog.getByText('高级设置',{exact:true}).click();
 return dialog;
}

test('Fast belongs to each provider; saving the active provider prompts restart',async({page})=>{
 await setupFast(page);await page.goto('/');
 await page.getByRole('button',{name:'设置',exact:true}).click();
 await expect(page.getByRole('checkbox',{name:'解锁原生Fast',exact:true})).toHaveCount(0);
 const dialog=await edit(page),toggle=dialog.getByRole('checkbox',{name:'解锁原生Fast',exact:true});
 await expect(toggle).not.toBeChecked();
 await expect(page.locator('.provider-card').getByText('Fast模式已启用',{exact:true})).toHaveCount(0);
 await expect(dialog.getByRole('checkbox',{name:'兼容图片工具',exact:true})).toBeVisible();
 await toggle.check();
 await expect(dialog.getByText('支持 Fast 的模型',{exact:true})).toHaveCount(0);
 await dialog.screenshot({path:'artifacts/screenshots/provider-fast.png'});
 await dialog.getByRole('button',{name:'保存配置',exact:true}).click();
 const confirm=page.getByRole('dialog',{name:'重启 Codex 后生效'});
 await expect(confirm).toContainText('正在生成的任务会中断');
 await confirm.getByRole('button',{name:'重启 Codex',exact:true}).click();
 await expect.poll(()=>page.evaluate(()=>(window as any).fastTest.restarts)).toEqual([true]);
 expect(await page.evaluate(()=>(window as any).fastTest.providers[1].nativeFastUnlock)).toBe(false);
 await expect(page.locator('.provider-card').getByText('Fast模式已启用',{exact:true})).toHaveCount(1);
 await page.screenshot({path:'artifacts/screenshots/provider-fast-status.png'});
 const reopened=await edit(page);
 await expect(reopened.getByRole('checkbox',{name:'解锁原生Fast',exact:true})).toBeChecked();
 await expect(reopened.getByRole('checkbox',{name:'加入 codex-main',exact:true})).toBeChecked();
 await reopened.getByRole('checkbox',{name:'解锁原生Fast',exact:true}).uncheck();
 await reopened.getByRole('button',{name:'保存配置',exact:true}).click();
 await confirm.getByRole('button',{name:'稍后',exact:true}).click();
 expect(await page.evaluate(()=>(window as any).fastTest.restarts)).toEqual([true]);
 expect(await page.evaluate(()=>(window as any).fastTest.providers[0].nativeFastUnlock)).toBe(false);
 await expect(page.locator('.provider-card').getByText('Fast模式已启用',{exact:true})).toHaveCount(0);
});

test('Inactive provider saves without restarting; cancel discards Fast changes',async({page})=>{
 await setupFast(page);await page.goto('/');
 let dialog=await edit(page,'另一供应商');
 await dialog.getByRole('checkbox',{name:'解锁原生Fast',exact:true}).check();
 await dialog.getByRole('button',{name:'取消',exact:true}).click();
 dialog=await edit(page,'另一供应商');
 await expect(dialog.getByRole('checkbox',{name:'解锁原生Fast',exact:true})).not.toBeChecked();
 await dialog.getByRole('checkbox',{name:'解锁原生Fast',exact:true}).check();
 await dialog.getByRole('button',{name:'保存配置',exact:true}).click();
 await expect(dialog).not.toBeVisible();
 await expect(page.getByRole('dialog',{name:'重启 Codex 后生效'})).toHaveCount(0);
 expect(await page.evaluate(()=>(window as any).fastTest.restarts)).toEqual([]);
 expect(await page.evaluate(()=>(window as any).fastTest.providers.map((p:any)=>p.nativeFastUnlock))).toEqual([false,true]);
});

test('Provider save failure keeps edits and does not offer restart',async({page})=>{
 await setupFast(page,{failSave:true});await page.goto('/');
 const dialog=await edit(page);
 await dialog.getByRole('checkbox',{name:'解锁原生Fast',exact:true}).check();
 await dialog.getByRole('button',{name:'保存配置',exact:true}).click();
 await expect(dialog).toContainText('模拟保存失败');
 await expect(page.getByRole('dialog',{name:'重启 Codex 后生效'})).toHaveCount(0);
 expect(await page.evaluate(()=>(window as any).fastTest.providers[0].nativeFastUnlock)).toBe(false);
 await page.evaluate(()=>{(window as any).fastTest.failSave=false});
 await dialog.getByRole('button',{name:'保存配置',exact:true}).click();
 await expect(page.getByRole('dialog',{name:'重启 Codex 后生效'})).toBeVisible();
});

test('Chat Completions cannot enable native Fast',async({page})=>{
 await setupFast(page,{active:false});await page.goto('/');
 const dialog=await edit(page);
 await dialog.getByLabel('协议').selectOption('ChatCompletions');
 await expect(dialog.getByRole('checkbox',{name:'解锁原生Fast',exact:true})).toBeDisabled();
});


test('Compact Fast layout stays readable in both themes and window sizes',async({page})=>{
 await setupFast(page);await page.goto('/');
 for(const [theme,width,height] of [['light',1180,820],['dark',940,620]] as const){
  await page.setViewportSize({width,height});
  await page.getByRole('button',{name:'设置',exact:true}).click();
  await page.getByRole('button',{name:theme==='light'?'浅色':'深色',exact:true}).click();
  await expect(page.getByTestId('settings-save-state')).toHaveText('已自动保存');
  await page.getByRole('button',{name:'仪表盘',exact:true}).click();
  await page.screenshot({path:`artifacts/screenshots/compact-dashboard-${theme}.png`});
  const dialog=await edit(page);
  await expect(dialog.getByText('支持 Fast 的模型',{exact:true})).toHaveCount(0);
  const options=dialog.locator('.provider-options .check-label');
  const [first,second]=await Promise.all([options.nth(0).boundingBox(),options.nth(1).boundingBox()]);
  expect(first).not.toBeNull();expect(second).not.toBeNull();
  expect(Math.abs(first!.y-second!.y)).toBeLessThan(1);expect(second!.x).toBeGreaterThan(first!.x+first!.width);
  await expect(dialog.locator('.provider-options svg')).toHaveCount(0);
  const context=await dialog.getByLabel('上下文窗口',{exact:true}).boundingBox();expect(first!.y).toBeLessThan(context!.y);
  await expect(dialog.getByText('使用说明',{exact:true})).toHaveCount(0);
  for(const option of await dialog.locator('.provider-option').all())await expect(option).toHaveCSS('background-color','rgba(0, 0, 0, 0)');
  await expect(dialog.locator('.provider-option .muted')).toHaveCount(0);
  await dialog.locator('.provider-form').evaluate(el=>{el.scrollTop=el.scrollHeight});
  await expect(dialog.getByRole('checkbox',{name:'解锁原生Fast',exact:true})).toBeInViewport();
  await expect(dialog.getByLabel('上下文窗口',{exact:true})).toBeInViewport();
  expect(await dialog.locator('.provider-form').evaluate(el=>el.scrollWidth<=el.clientWidth)).toBe(true);
  await dialog.screenshot({path:`artifacts/screenshots/compact-editor-${theme}.png`});
  await dialog.getByRole('button',{name:'取消',exact:true}).click();
 }
});


test('Sidebar folding keeps navigation accessible and persists across reloads',async({page})=>{
 await page.goto('/');
 await expect(page.locator('.sidebar')).toHaveCSS('border-right-width','0px');
 await page.getByRole('button',{name:'收起导航',exact:true}).click();
 await expect(page.locator('.app-shell')).toHaveClass(/sidebar-collapsed/);
 await expect(page.locator('.sidebar')).toHaveCSS('border-right-width','0px');
 await expect(page.locator('.app-shell>main')).toHaveCSS('border-top-left-radius','16px');
 await page.getByRole('button',{name:'设置',exact:true}).click();
 await expect(page.getByRole('heading',{name:'设置',exact:true})).toBeVisible();
 await page.reload();
 await expect(page.locator('.app-shell')).toHaveClass(/sidebar-collapsed/);
 await page.getByRole('button',{name:'供应商',exact:true}).click();
 await expect(page.locator('.provider-card')).toHaveCount(4);
 await page.setViewportSize({width:940,height:620});
 expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
 await page.screenshot({path:'artifacts/screenshots/sidebar-folded.png'});
 await page.getByRole('button',{name:'展开导航',exact:true}).click();
 await expect(page.locator('.app-shell')).not.toHaveClass(/sidebar-collapsed/);
});


test('Sidebar motion interpolates, reverses and respects reduced motion',async({page},testInfo)=>{
 await page.emulateMedia({reducedMotion:'no-preference'});
 await page.setViewportSize({width:1180,height:840});await page.goto('/');
 await expect(page.getByRole('heading',{name:'仪表盘',exact:true})).toBeVisible();
 const width=()=>page.locator('.sidebar').evaluate(el=>el.getBoundingClientRect().width);
 const expanded=await width();
 await page.getByRole('button',{name:'收起导航',exact:true}).click();
 await expect.poll(width).toBeCloseTo(64,0);
 const frames=await page.evaluate(async()=>{
  const rail=document.querySelector('.sidebar')!;
  (document.querySelector('.sidebar-toggle') as HTMLButtonElement).click();
  const start=performance.now(),frames:{time:number;width:number}[]=[];
  await new Promise<void>(resolve=>{const sample=()=>{const time=performance.now()-start;frames.push({time,width:rail.getBoundingClientRect().width});if(time<360)requestAnimationFrame(sample);else resolve()};requestAnimationFrame(sample)});
  return frames;
 });
 expect(frames.some(f=>f.width>65&&f.width<expanded-1)).toBe(true);
 expect(frames.at(-1)!.width).toBeCloseTo(expanded,0);
 const middle=frames.find(f=>f.time>=100&&f.time<=200);
 expect(middle).toBeDefined();expect((middle!.width-64)/(expanded-64)).toBeGreaterThan(middle!.time/300);
 await testInfo.attach('sidebar-motion',{body:JSON.stringify(frames),contentType:'application/json'});
 await page.evaluate(async()=>{const toggle=document.querySelector('.sidebar-toggle') as HTMLButtonElement;toggle.click();await new Promise(requestAnimationFrame);await new Promise(requestAnimationFrame);toggle.click()});
 await expect.poll(width).toBeCloseTo(expanded,0);
 await page.getByRole('button',{name:'收起导航',exact:true}).focus();await page.keyboard.press('Enter');await expect.poll(width).toBeCloseTo(64,0);
 await page.emulateMedia({reducedMotion:'reduce'});
 await page.getByRole('button',{name:'展开导航',exact:true}).click();
 expect(await width()).toBeCloseTo(expanded,0);
 expect(await page.locator('.app-shell').evaluate(el=>el.getAnimations().length)).toBe(0);
 await page.getByRole('button',{name:'设置',exact:true}).click();await expect(page.getByRole('heading',{name:'软件更新',exact:true})).toBeVisible();
 await page.screenshot({path:'artifacts/screenshots/window-1180x840.png',animations:'disabled'});
});
