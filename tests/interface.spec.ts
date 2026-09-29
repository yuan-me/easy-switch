import {test,expect} from '@playwright/test';
test('overlapping session refreshes share a request and release it afterwards',async({page})=>{
 await page.addInitScript(()=>{
  let calls=0,resolveScan:(v:unknown)=>void=()=>{};
  Object.assign(window,{isTauri:true,__TAURI_EVENT_PLUGIN_INTERNALS__:{unregisterListener:()=>{}},scanCalls:()=>calls,finishScan:()=>resolveScan({sessions:[],warnings:[]}),__TAURI_INTERNALS__:{
   transformCallback:()=>1,
   invoke:async(command:string)=>{
    if(command==='bootstrap')return {providers:[],settings:{theme:'light',scrollPositions:{}},version:'test',migrationError:null};
    if(command==='scan_sessions'){calls++;return new Promise(resolve=>{resolveScan=resolve})}
    return 1;
   }
  }});
 });
 await page.goto('/');await page.getByRole('button',{name:'会话',exact:true}).click();
 const calls=()=>page.evaluate(()=>(window as unknown as {scanCalls:()=>number}).scanCalls());
 await expect.poll(calls).toBe(1);
 await page.getByRole('button',{name:'刷新',exact:true}).click();
 await page.evaluate(()=>(window as unknown as {finishScan:()=>void}).finishScan());
 await expect(page.getByRole('button',{name:'刷新',exact:true})).toBeEnabled();expect(await calls()).toBe(1);
 await page.getByRole('button',{name:'刷新',exact:true}).click();await expect.poll(calls).toBe(2);
 await page.evaluate(()=>(window as unknown as {finishScan:()=>void}).finishScan());
 await expect(page.getByRole('button',{name:'刷新',exact:true})).toBeEnabled();
});
test('provider navigation, search, themes, editor and minimum viewport',async({page})=>{
 const errors:string[]=[];page.on('pageerror',e=>errors.push(e.message));await page.goto('/');await expect(page.getByRole('heading',{name:'供应商',exact:true})).toBeVisible();await expect(page.locator('.provider-card')).toHaveCount(4);
 await page.getByLabel('浅色',{exact:true}).click();await page.screenshot({animations:'disabled',path:'artifacts/screenshots/providers-light.png',fullPage:true});
 await page.getByLabel('搜索供应商').fill('Sub2API');await expect(page.locator('.provider-card')).toHaveCount(1);await page.getByRole('button',{name:'设置',exact:true}).click();await page.getByRole('button',{name:'供应商',exact:true}).click();await expect(page.getByLabel('搜索供应商')).toHaveValue('Sub2API');await page.getByLabel('搜索供应商').fill('');
 await page.getByRole('button',{name:'添加供应商',exact:true}).click();await expect(page.getByRole('dialog')).toBeVisible();await page.getByLabel('显示名称',{exact:true}).fill('测试供应商');await page.getByLabel('Sub2API 图片工具兼容').check();await expect(page.getByLabel('Sub2API 图片工具兼容')).toBeChecked();await page.screenshot({animations:'disabled',path:'artifacts/screenshots/provider-editor.png',fullPage:true});await page.keyboard.press('Escape');await expect(page.getByRole('dialog')).not.toBeVisible();
 await page.getByLabel('深色',{exact:true}).click();await expect(page.locator('html')).toHaveAttribute('data-theme','dark');await page.screenshot({animations:'disabled',path:'artifacts/screenshots/providers-dark.png',fullPage:true});
 await page.getByRole('button',{name:'会话',exact:true}).click();await expect(page.getByText('选择一个会话',{exact:true})).toBeVisible();await page.screenshot({animations:'disabled',path:'artifacts/screenshots/sessions-dark.png',fullPage:true});
 await page.getByRole('button',{name:'设置',exact:true}).click();await expect(page.getByRole('heading',{name:'软件更新',exact:true})).toBeVisible();await page.screenshot({animations:'disabled',path:'artifacts/screenshots/settings-dark.png',fullPage:true});
 await page.setViewportSize({width:940,height:620});await page.getByLabel('浅色',{exact:true}).click();await page.getByRole('button',{name:'供应商',exact:true}).click();await page.screenshot({animations:'disabled',path:'artifacts/screenshots/providers-min.png',fullPage:true});
 expect(await page.evaluate(()=>document.documentElement.scrollWidth<=window.innerWidth)).toBe(true);expect(errors).toEqual([]);
});
test('each scale remains usable and keyboard focus is visible',async({browser})=>{for(const scale of [1,1.25,1.5,2]){const context=await browser.newContext({viewport:{width:1120,height:780},deviceScaleFactor:scale,colorScheme:'light'});const page=await context.newPage();await page.goto('/');await expect(page.getByRole('heading',{name:'供应商',exact:true})).toBeVisible();await page.keyboard.press('Tab');expect(await page.evaluate(()=>document.activeElement?.tagName)).toBe('BUTTON');await page.screenshot({animations:'disabled',path:`artifacts/screenshots/providers-scale-${scale}.png`});await context.close();}});
