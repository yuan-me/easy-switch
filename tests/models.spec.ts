import {test,expect,type Page} from '@playwright/test';
import {join} from 'node:path';
import {tmpdir} from 'node:os';

async function desktop(page:Page){
 await page.addInitScript(()=>{
  const w=window as any, callbacks=new Map<number,Function>(), listeners=new Map<string,Function>();let callback=0;
  const provider={id:'api',name:'hi',mode:'Api',protocol:'Responses',model:'gpt-6-sol',baseUrl:'https://example.invalid/v1',headers:{},hasKey:true,members:[],strategy:'Failover',imageCompatibility:true};
  const settings={theme:'light',activeProviderId:'api',scrollPositions:{}};
  const lists:Record<string,any>={api:{available:['gpt-6-sol','gpt-6.1-sol','excluded-model'],excluded:['excluded-model'],added:['gpt-6.1-sol'],removed:[],checkedAt:1800000000,error:null}};
  w.modelCalls=[];w.modelLists=lists;
  w.newModel=()=>{lists.api.available.push('gpt-new');lists.api.added.push('gpt-new');listeners.get('models-updated')?.({payload:structuredClone(lists)})};
  Object.assign(w,{isTauri:true,__TAURI_EVENT_PLUGIN_INTERNALS__:{unregisterListener:()=>{}},__TAURI_INTERNALS__:{
   metadata:{currentWindow:{label:'main'},currentWebview:{label:'main'}},
   transformCallback:(fn:Function)=>{callbacks.set(++callback,fn);return callback},
   invoke:async(command:string,args:any)=>{
    w.modelCalls.push({command,args});
    if(command==='plugin:event|listen'){listeners.set(args.event,callbacks.get(args.handler)!);return args.handler}
    if(command==='interrupted_operations')return [];
    if(command==='bootstrap')return {providers:[provider],modelCatalogs:structuredClone(lists),settings,version:'1.1.2',migrationError:null};
    if(command==='window_appearance')return false;
    if(command==='plugin:window|is_maximized')return false;
    if(command==='save_settings'){Object.assign(settings,args.settings);return}
    if(command==='reveal_provider_key'){if(w.failKey)throw new Error('synthetic credential failure');return 'synthetic-saved-key'}
    if(command==='fetch_models'){if(w.failModels)throw new Error('模型列表 HTTP 503');return ['gpt-6-sol','gpt-6.1-sol','excluded-model','fresh-model']}
    if(command==='refresh_provider_models'){if(w.failModels)throw new Error('模型列表 HTTP 503');return structuredClone(lists.api)}
    if(command==='acknowledge_models'){lists.api.added=[];lists.api.removed=[];return}
    if(command==='save_provider'){if(w.failSave)throw new Error('模拟保存失败');Object.assign(provider,args.provider);if(args.modelSelection){lists.api.available=args.modelSelection.available;lists.api.excluded=args.modelSelection.excluded}return}
    return null;
   }
  }});
 });
 await page.goto('/');
 await expect(page.getByRole('heading',{name:'供应商',exact:true})).toBeVisible();
}

test('new-model badge is a label and editing persists model choices',async({page})=>{
 const errors:string[]=[];page.on('pageerror',e=>errors.push(e.message));
 await desktop(page);
 await expect(page).toHaveTitle('Easy Switch');
 await expect(page.locator('vite-error-overlay')).toHaveCount(0);
 await expect(page.locator('.model-notice')).toContainText('NEW');
 await expect(page.locator('.model-notice')).toContainText('gpt-6.1-sol');
 await page.screenshot({animations:'disabled',path:join(tmpdir(),'easy-switch-models-new.png')});
 await page.locator('.provider-card').screenshot({animations:'disabled',path:join(tmpdir(),'easy-switch-models-card.png')});
 await expect(page.locator('.model-notice')).toHaveJSProperty('tagName','SPAN');
 await page.locator('.model-notice').click();await expect(page.getByRole('dialog')).toHaveCount(0);
 await page.getByRole('button',{name:'编辑',exact:true}).click();
 const dialog=page.getByRole('dialog');
 await expect(dialog.getByLabel('加入 gpt-6-sol',{exact:true})).toBeDisabled();
 await expect(dialog.getByLabel('加入 excluded-model')).not.toBeChecked();
 await dialog.getByLabel('加入 gpt-6.1-sol').uncheck();
 await page.screenshot({animations:'disabled',path:join(tmpdir(),'easy-switch-models-picker.png')});
 await dialog.getByRole('button',{name:'保存配置'}).click();
 await expect(dialog).not.toBeVisible();
 expect(await page.evaluate(()=>(window as any).modelLists.api.excluded)).toEqual(['excluded-model','gpt-6.1-sol']);
 await expect(page.locator('.model-notice')).toHaveCount(0);
 await page.evaluate(()=>(window as any).newModel());
 await expect(page.locator('.model-notice')).toContainText('gpt-new');
 await page.getByLabel('深色',{exact:true}).click();
 await page.setViewportSize({width:940,height:620});
 await expect(page.locator('html')).toHaveAttribute('data-theme','dark');
 await expect(page.locator('.provider-card')).toHaveCSS('background-color','rgb(31, 39, 52)');
 await page.screenshot({animations:'disabled',path:join(tmpdir(),'easy-switch-models-dark-min.png')});
 expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
 expect(errors).toEqual([]);
});

test('editor fetch exposes selectable models and submits choices with provider',async({page})=>{
 await desktop(page);await page.getByRole('button',{name:'编辑',exact:true}).click();
 const dialog=page.getByRole('dialog');
 await dialog.getByRole('button',{name:'获取模型列表'}).click();
 await expect(dialog.getByLabel('加入 fresh-model')).toBeChecked();
 await dialog.getByLabel('加入 fresh-model').uncheck();
 await dialog.getByLabel('加入 excluded-model').check();
 await dialog.getByRole('button',{name:'保存配置'}).click();
 await expect(dialog).not.toBeVisible();
 const saved=await page.evaluate(()=>(window as any).modelCalls.find((c:any)=>c.command==='save_provider').args);
 expect(saved.modelSelection.available).toContain('fresh-model');
 expect(saved.modelSelection.excluded).toEqual(['fresh-model']);
});

test('failed refresh keeps previous choices and displays an error',async({page})=>{
 await desktop(page);await page.getByRole('button',{name:'编辑',exact:true}).click();
 await page.evaluate(()=>(window as any).failModels=true);
 const dialog=page.getByRole('dialog');await dialog.getByRole('button',{name:'获取模型列表'}).click();
 await expect(dialog.getByRole('alert')).toContainText('503');
 await expect(dialog.getByLabel('加入 gpt-6.1-sol')).toBeChecked();
 await expect(dialog.getByLabel('加入 excluded-model')).not.toBeChecked();
});

test('model search stays inline and nested scrollbars appear only during scrolling',async({page})=>{
 await desktop(page);
 await page.setViewportSize({width:940,height:620});
 await page.evaluate(()=>{
  (window as any).modelLists.api.available.push(...Array.from({length:28},(_,i)=>`test-model-${String(i).padStart(2,'0')}`));
  (window as any).newModel();
 });
 {
  await page.getByRole('button',{name:'编辑',exact:true}).click();
  const dialog=page.getByRole('dialog'),search=dialog.getByLabel('搜索模型'),choices=dialog.locator('.model-choices');
  await search.scrollIntoViewIfNeeded();
  await search.focus();
  const layout=await search.evaluate(input=>{
   const label=input.parentElement!,icon=label.querySelector('svg')!;
   const field=input.getBoundingClientRect(),glyph=icon.getBoundingClientRect();
   return {height:label.getBoundingClientRect().height,offset:Math.abs(field.y+field.height/2-glyph.y-glyph.height/2),separated:glyph.right<field.left,border:getComputedStyle(input).borderWidth};
  });
  expect(layout.height).toBeLessThanOrEqual(42);expect(layout.offset).toBeLessThan(2);expect(layout.separated).toBe(true);expect(layout.border).toBe('0px');
  await expect(search.locator('..')).toHaveCSS('outline-style','solid');
  await expect(choices).toHaveCSS('scrollbar-color','rgba(0, 0, 0, 0) rgba(0, 0, 0, 0)');
  const width=await choices.evaluate(el=>el.clientWidth);
  await choices.hover();await page.mouse.wheel(0,170);
  await expect(choices).toHaveAttribute('data-scrolling','true');
  await expect(choices).not.toHaveCSS('scrollbar-color','rgba(0, 0, 0, 0) rgba(0, 0, 0, 0)');
  expect(await choices.evaluate(el=>el.clientWidth)).toBe(width);
  await expect(choices).not.toHaveAttribute('data-scrolling');
  await expect(choices).toHaveCSS('scrollbar-color','rgba(0, 0, 0, 0) rgba(0, 0, 0, 0)');
  await expect(dialog.locator('.modal-footer')).toBeInViewport();
  const bounds=await dialog.boundingBox();expect(bounds!.x).toBeGreaterThan(0);expect(bounds!.y).toBeGreaterThan(0);expect(bounds!.y+bounds!.height).toBeLessThan(620);
  await page.screenshot({animations:'disabled',path:join(tmpdir(),'easy-switch-islands-editor-min.png')});
  await search.fill('test-model-01');await expect(choices.locator('.model-choice')).toHaveCount(1);
  await page.keyboard.press('Escape');
 }
 await page.getByLabel('深色',{exact:true}).click();await page.getByRole('button',{name:'编辑',exact:true}).click();
 await page.getByLabel('搜索模型').scrollIntoViewIfNeeded();
 await page.screenshot({animations:'disabled',path:join(tmpdir(),'easy-switch-islands-editor-dark-min.png')});
 await page.emulateMedia({forcedColors:'active'});
 await expect(page.locator('.model-choices')).toHaveCSS('scrollbar-color','auto');
});


test('model dropdown matches other menus while allowing a custom model ID',async({page})=>{
 await desktop(page);await page.setViewportSize({width:940,height:620});
 for(const [theme,color] of [['浅色','rgb(255, 255, 255)'],['深色','rgb(31, 39, 52)']]){
  await page.getByLabel(theme,{exact:true}).click();await page.getByRole('button',{name:'编辑',exact:true}).click();
  const dialog=page.getByRole('dialog'),models=dialog.getByLabel('选择默认模型');
  await expect(models.locator('selectedcontent')).toHaveCSS('display','none');
  await models.click();
  expect(await models.evaluate(el=>getComputedStyle(el,'::picker(select)').backgroundColor)).toBe(color);
  await page.screenshot({path:`artifacts/screenshots/dropdown-${theme}-models.png`});
  await page.keyboard.press('Escape');await expect(dialog).toBeVisible();
  await models.selectOption('excluded-model');await expect(dialog.getByLabel('默认模型',{exact:true})).toHaveValue('excluded-model');
  await expect(dialog.getByLabel('加入 excluded-model')).toBeChecked();await expect(dialog.getByLabel('加入 excluded-model')).toBeDisabled();
  await dialog.getByLabel('默认模型',{exact:true}).fill('custom-unlisted-model');await expect(models).toHaveValue('');
  await dialog.getByRole('button',{name:'保存配置'}).click();await expect(dialog).not.toBeVisible();
 }
 const saves=await page.evaluate(()=>(window as any).modelCalls.filter((c:any)=>c.command==='save_provider'));
 expect(saves).toHaveLength(2);expect(saves[1].args.provider.model).toBe('custom-unlisted-model');
});


test('one editor owns defaults, refresh time and choices; cancel preserves the saved state',async({page})=>{
 await desktop(page);await page.getByRole('button',{name:'编辑',exact:true}).click();
 const dialog=page.getByRole('dialog');
 await expect(dialog.getByText('模型列表自动同步')).toHaveCount(0);
 await expect(dialog.getByText('Easy Switch 启动时及运行期间每小时更新')).toHaveCount(0);
 await expect(dialog.getByText('保存后自动更新 Codex 模型目录；运行中的 Codex 可能需要下次启动才显示。')).toHaveCount(0);
 await expect(dialog.getByText(/已获取.*个模型/)).toHaveCount(0);
 await expect(dialog.locator('.model-updated')).toContainText('上次更新');
 await dialog.locator('.form-scroll').evaluate(el=>{const label=el.querySelector('.model-refresh')!.parentElement!;el.scrollTop+=label.getBoundingClientRect().top-el.getBoundingClientRect().top-16});
 await page.screenshot({animations:'disabled',path:'artifacts/screenshots/unified-editor-models.png'});

 await dialog.getByLabel('选择默认模型').selectOption('excluded-model');
 await dialog.getByLabel('加入 gpt-6-sol',{exact:true}).uncheck();
 const before=await dialog.locator('.model-updated').textContent();
 await dialog.getByRole('button',{name:'获取模型列表'}).click();
 await expect(dialog.getByLabel('默认模型',{exact:true})).toHaveValue('excluded-model');
 await expect(dialog.getByLabel('加入 gpt-6-sol',{exact:true})).not.toBeChecked();
 await expect(dialog.getByLabel('加入 excluded-model')).toBeDisabled();
 await expect(dialog.locator('.model-updated')).not.toHaveText(before!);
 await dialog.getByRole('button',{name:'取消',exact:true}).click();
 expect(await page.evaluate(()=>(window as any).modelCalls.filter((c:any)=>c.command==='save_provider'))).toEqual([]);
 await page.getByRole('button',{name:'编辑',exact:true}).click();
 await expect(dialog.getByLabel('默认模型',{exact:true})).toHaveValue('gpt-6-sol');
 await expect(dialog.getByLabel('加入 excluded-model')).not.toBeChecked();
});

test('changing default includes it, allows removing the old default, and survives save retry',async({page})=>{
 await desktop(page);await page.getByRole('button',{name:'编辑',exact:true}).click();
 const dialog=page.getByRole('dialog');await dialog.getByLabel('选择默认模型').selectOption('excluded-model');
 await expect(dialog.getByLabel('加入 excluded-model')).toBeChecked();await expect(dialog.getByLabel('加入 excluded-model')).toBeDisabled();
 await dialog.getByLabel('加入 gpt-6-sol',{exact:true}).uncheck();
 await page.evaluate(()=>(window as any).failSave=true);await dialog.getByRole('button',{name:'保存配置'}).click();
 await expect(dialog.getByRole('alert')).toContainText('保存失败');await expect(dialog.getByLabel('默认模型',{exact:true})).toHaveValue('excluded-model');
 await expect(dialog.getByLabel('加入 gpt-6-sol',{exact:true})).not.toBeChecked();
 await page.evaluate(()=>(window as any).failSave=false);await dialog.getByRole('button',{name:'保存配置'}).click();await expect(dialog).not.toBeVisible();
 await expect(page.locator('.provider-card-body')).toContainText('excluded-model');
 await page.getByRole('button',{name:'编辑',exact:true}).click();await expect(dialog.getByLabel('默认模型',{exact:true})).toHaveValue('excluded-model');
 await expect(dialog.getByLabel('加入 gpt-6-sol',{exact:true})).not.toBeChecked();
});

test('select all includes models outside the search and preserves unavailable exclusions',async({page})=>{
 await desktop(page);
 await page.evaluate(()=>{const w=window as any;w.modelLists.api.excluded.push('gpt-6.1-sol','unavailable-model');w.newModel()});
 await page.getByRole('button',{name:'编辑',exact:true}).click();
 const dialog=page.getByRole('dialog');
 await dialog.getByLabel('搜索模型').fill('excluded-model');
 await expect(dialog.locator('.model-choice')).toHaveCount(1);
 await dialog.getByRole('button',{name:'全选',exact:true}).click();
 await expect(dialog.getByLabel('加入 excluded-model')).toBeChecked();
 await expect(dialog.getByRole('button',{name:'取消全选',exact:true})).toBeEnabled();
 await dialog.getByLabel('搜索模型').clear();
 await expect(dialog.locator('.model-choice input:checked')).toHaveCount(4);
 await expect(dialog.getByLabel('加入 gpt-6-sol',{exact:true})).toBeDisabled();
 await dialog.getByRole('button',{name:'保存配置'}).click();await expect(dialog).not.toBeVisible();
 expect(await page.evaluate(()=>(window as any).modelLists.api.excluded)).toEqual(['unavailable-model']);
 await page.getByRole('button',{name:'编辑',exact:true}).click();
 await expect(dialog.getByRole('button',{name:'取消全选',exact:true})).toBeEnabled();
 await dialog.getByRole('button',{name:'取消全选',exact:true}).click();
 await expect(dialog.locator('.model-choice input:checked')).toHaveCount(1);
 await expect(dialog.getByLabel('加入 gpt-6-sol',{exact:true})).toBeChecked();
 await dialog.getByRole('button',{name:'保存配置'}).click();await expect(dialog).not.toBeVisible();
 expect(await page.evaluate(()=>(window as any).modelLists.api.excluded)).toEqual(['unavailable-model','excluded-model','gpt-6.1-sol','gpt-new']);
 await page.getByRole('button',{name:'编辑',exact:true}).click();
 await dialog.getByRole('button',{name:'全选',exact:true}).click();
 await expect(dialog.locator('.model-choice input:checked')).toHaveCount(4);
 await dialog.getByLabel('加入 gpt-6.1-sol').uncheck();await expect(dialog.getByRole('button',{name:'全选',exact:true})).toBeEnabled();
});

test('editor is centered and grouped with two model columns at desktop and minimum sizes',async({page})=>{
 const errors:string[]=[];page.on('pageerror',e=>errors.push(e.message));
 await desktop(page);
 for(const [theme,width,height] of [['浅色',1120,780],['深色',940,620]] as const){
  await page.setViewportSize({width,height});await page.getByLabel(theme,{exact:true}).click();
  await page.getByRole('button',{name:'编辑',exact:true}).click();
  const dialog=page.getByRole('dialog');
  await expect(dialog.getByRole('heading',{name:'基本信息'})).toBeVisible();
  await expect(dialog.getByRole('heading',{name:'连接设置'})).toBeVisible();
  await expect(dialog.getByRole('heading',{name:'模型设置',exact:true})).toBeVisible();
  await expect(dialog.locator('.provider-advanced')).not.toHaveAttribute('open');
  await expect(dialog.getByLabel('上下文窗口')).not.toBeVisible();
  await expect(dialog).toHaveCSS('transform','matrix(1, 0, 0, 1, 0, 0)');
  await expect(dialog.getByLabel('Provider ID',{exact:true})).toHaveCount(0);
  const basic=dialog.locator('section').filter({has:page.getByRole('heading',{name:'基本信息'})});
  const connection=dialog.locator('section').filter({has:page.getByRole('heading',{name:'连接设置'})});
  const basicBox=await basic.boundingBox(),connectionBox=await connection.boundingBox();
  expect(connectionBox!.y).toBeGreaterThanOrEqual(basicBox!.y+basicBox!.height);
  await expect(connection).toHaveCSS('border-width','1px');
  const protocol=await dialog.getByLabel('协议').boundingBox(),address=await dialog.getByLabel('API 地址').boundingBox(),key=await dialog.getByLabel('API Key',{exact:true}).boundingBox();
  expect(Math.abs(protocol!.y-address!.y)).toBeLessThan(1);expect(address!.x).toBeGreaterThan(protocol!.x);
  expect(key!.y).toBeGreaterThan(address!.y);expect(key!.width).toBeGreaterThan(address!.width);
  await expect(dialog.getByPlaceholder('留空保留已有密钥')).toHaveCount(0);
  const bounds=await dialog.boundingBox();expect(bounds!.width).toBeGreaterThan(850);
  expect(Math.abs(bounds!.x-(width-bounds!.width)/2)).toBeLessThan(1);
  expect(Math.abs(bounds!.y-(height-bounds!.height)/2)).toBeLessThan(1);
  await page.screenshot({animations:'disabled',path:join(tmpdir(),`easy-switch-sections-${theme}.png`)});
  await dialog.getByLabel('搜索模型').scrollIntoViewIfNeeded();
  const cells=dialog.locator('.model-choice');
  const first=await cells.nth(0).boundingBox(),second=await cells.nth(1).boundingBox();
  expect(Math.abs(first!.y-second!.y)).toBeLessThan(1);expect(second!.x).toBeGreaterThanOrEqual(first!.x+first!.width);
  expect(await dialog.evaluate(el=>el.scrollWidth<=el.clientWidth)).toBe(true);
  await expect(dialog.locator('.modal-footer')).toBeInViewport();
  await page.screenshot({animations:'disabled',path:join(tmpdir(),`easy-switch-model-columns-${theme}.png`)});
  await page.keyboard.press('Escape');
 }
 expect(errors).toEqual([]);
});

test('collapsed advanced fields reopen for validation and retain their values when saved',async({page})=>{
 await desktop(page);await page.getByRole('button',{name:'编辑',exact:true}).click();
 const dialog=page.getByRole('dialog'),advanced=dialog.locator('.provider-advanced');
 await advanced.locator('summary').click();await dialog.getByLabel('上下文窗口').fill('0');
 await advanced.locator('summary').click();await dialog.getByRole('button',{name:'保存配置'}).click();
 await expect(advanced).toHaveAttribute('open','');await expect(dialog.getByLabel('上下文窗口')).toBeFocused();
 expect(await page.evaluate(()=>(window as any).modelCalls.filter((c:any)=>c.command==='save_provider'))).toHaveLength(0);
 await dialog.getByLabel('上下文窗口').fill('64000');await dialog.getByLabel('自动压缩阈值').fill('8000');
 await dialog.getByLabel('自定义请求头（JSON，空值保留）').fill('{"X-Test":"retained"}');
 await dialog.getByLabel('Sub2API 图片工具兼容').uncheck();await advanced.locator('summary').click();
 await dialog.getByRole('button',{name:'保存配置'}).click();await expect(dialog).not.toBeVisible();
 const saved=await page.evaluate(()=>(window as any).modelCalls.find((c:any)=>c.command==='save_provider').args.provider);
 expect(saved).toMatchObject({contextWindow:64000,compactLimit:8000,headers:{'X-Test':'retained'},imageCompatibility:false});
});


test('API key reveal is explicit, reversible and does not rewrite an unchanged key',async({page})=>{
 await desktop(page);await page.getByRole('button',{name:'编辑',exact:true}).click();
 const dialog=page.getByRole('dialog'),key=dialog.getByLabel('API Key',{exact:true});
 await expect(key).toHaveValue('');await expect(key).toHaveAttribute('type','password');
 const reads=()=>page.evaluate(()=>(window as any).modelCalls.filter((c:any)=>c.command==='reveal_provider_key').length);
 expect(await reads()).toBe(0);
 await dialog.getByRole('button',{name:'显示 API Key',exact:true}).click();
 await expect(key).toHaveValue('synthetic-saved-key');await expect(key).toHaveAttribute('type','text');
 await dialog.getByRole('button',{name:'隐藏 API Key',exact:true}).click();await expect(key).toHaveAttribute('type','password');
 await dialog.getByRole('button',{name:'显示 API Key',exact:true}).click();expect(await reads()).toBe(1);
 await dialog.getByRole('button',{name:'保存配置'}).click();await expect(dialog).not.toBeVisible();
 expect(await page.evaluate(()=>(window as any).modelCalls.find((c:any)=>c.command==='save_provider').args.key)).toBe('');
 await page.getByRole('button',{name:'编辑',exact:true}).click();await expect(key).toHaveValue('');await expect(key).toHaveAttribute('type','password');
 await key.fill('synthetic-replacement-key');await dialog.getByRole('button',{name:'显示 API Key',exact:true}).click();
 await expect(key).toHaveValue('synthetic-replacement-key');expect(await reads()).toBe(1);
 await dialog.getByRole('button',{name:'保存配置'}).click();await expect(dialog).not.toBeVisible();
 expect(await page.evaluate(()=>(window as any).modelCalls.filter((c:any)=>c.command==='save_provider').at(-1).args.key)).toBe('synthetic-replacement-key');
});

test('failed key reveal stays hidden and permits retry',async({page})=>{
 await desktop(page);await page.getByRole('button',{name:'编辑',exact:true}).click();
 await page.evaluate(()=>(window as any).failKey=true);
 const dialog=page.getByRole('dialog'),key=dialog.getByLabel('API Key',{exact:true});
 await dialog.getByRole('button',{name:'显示 API Key',exact:true}).click();
 await expect(dialog.getByRole('alert')).toContainText('无法读取已保存的 API Key');
 await expect(key).toHaveValue('');await expect(key).toHaveAttribute('type','password');
 await page.evaluate(()=>(window as any).failKey=false);await dialog.getByRole('button',{name:'显示 API Key',exact:true}).click();
 await expect(key).toHaveValue('synthetic-saved-key');await expect(dialog.getByRole('alert')).toHaveCount(0);
});
