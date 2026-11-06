import {test,expect,type Page} from '@playwright/test';
import {join} from 'node:path';
import {tmpdir} from 'node:os';

async function desktop(page:Page){
 await page.clock.install({time:new Date('2026-10-19T15:00:00+08:00')});
 await page.addInitScript(()=>{
  const w=window as any;w.calls=[];w.failUsage=false;w.emptyUsage=false;
  const provider={id:'api',name:'开发主线路',mode:'Api',protocol:'Responses',baseUrl:'https://api.example.com/v1',model:'codex-main',members:[],headers:{},strategy:'Failover',hasKey:true};
  const settings={theme:'light',codexHome:'C:\\Demo',runtimePort:47831,scrollPositions:{},activeProviderId:'api',automaticUpdates:true,automaticDownload:true};
  const sessions=['仪表盘布局调整','接口错误排查','文档整理','单元测试补充'].map((title,i)=>({id:'session-'+i,title,cwd:'D:\\Projects\\codex-switch',provider:'api',updated:1792386000-i*3600,archived:false,path:'demo.jsonl',database:null}));
  Object.assign(w,{isTauri:true,__TAURI_EVENT_PLUGIN_INTERNALS__:{unregisterListener:()=>{}},__TAURI_INTERNALS__:{metadata:{currentWindow:{label:'main'},currentWebview:{label:'main'}},transformCallback:()=>1,
   invoke:async(command:string,args:any)=>{
    w.calls.push({command,args});
    if(command==='bootstrap')return {providers:[provider],settings,modelCatalogs:{api:{available:['codex-main','fast','new'],excluded:['fast'],added:['new'],removed:[],checkedAt:1792386000,error:null}},version:'1.3.0',migrationError:null};
    if(command==='save_settings'){Object.assign(settings,args.settings);return}
    if(command==='interrupted_operations')return [];
    if(command==='scan_sessions')return {sessions,warnings:[]};
    if(command==='session_detail')return {messages:[{role:'user',text:args.id,time:''}],tokens:[],relations:[],truncated:false,hasEncryptedContent:false};
    if(command==='usage_report'){
     if(w.failUsage)throw new Error('本地文件暂时不可读');
     const starts=args.query.bucketStarts,isDay=starts.length>7,input=w.emptyUsage?0:isDay?128400:960000,output=input/4,cached=w.emptyUsage?null:input*.6;
     return {input,output,cached,buckets:starts.map((start:number,i:number)=>({start,input:i===starts.length-1?input:0,output:i===starts.length-1?output:0,cached})),models:[],sessions:[],availableModels:[],scannedFiles:w.emptyUsage?0:4,warnings:[]};
    }
    return false;
   }
  }});
 });
 await page.goto('/');await expect(page.getByTestId('dashboard-input')).toHaveText('128.4K');
}

test('dashboard uses local today and rolling seven days, links to details, and retries independent failures',async({page})=>{
 const errors:string[]=[];page.on('pageerror',e=>errors.push(e.message));await desktop(page);
 await expect(page.locator('nav button')).toHaveText(['仪表盘','供应商','Token 统计','会话','设置']);
 await expect(page.locator('.sidebar-bottom,.theme-buttons')).toHaveCount(0);
 await expect(page.getByTestId('dashboard-output')).toHaveText('32.1K');
 await expect(page.locator('.daily-total')).toContainText('160.5K');
 await expect(page.locator('.daily-cache')).toContainText('60%');
 const queries=await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.command==='usage_report').map((c:any)=>c.args.query));
 const week=queries.find((q:any)=>q.bucketStarts.length===7),day=queries.find((q:any)=>q.bucketStarts.length===24);
 expect(week.bucketStarts[6]).toBe(day.bucketStarts[0]);expect(week.end).toBe(day.end);
 expect(week.bucketStarts[6]-week.bucketStarts[0]).toBe(6*86400);
 await page.getByRole('button',{name:'仪表盘布局调整',exact:true}).click();await expect(page.locator('.detail-heading h2')).toHaveText('仪表盘布局调整');
 await page.locator('.session-row button').filter({hasText:'接口错误排查'}).click();await expect(page.locator('.detail-heading h2')).toHaveText('接口错误排查');
 await page.getByRole('button',{name:'仪表盘',exact:true}).click();await page.getByRole('button',{name:'仪表盘布局调整',exact:true}).click();await expect(page.locator('.detail-heading h2')).toHaveText('仪表盘布局调整');
 await page.getByRole('button',{name:'仪表盘',exact:true}).click();await expect(page.getByTestId('dashboard-input')).toHaveText('128.4K');
 await page.getByRole('button',{name:'管理模型',exact:true}).click();await expect(page.getByRole('dialog',{name:'编辑供应商'})).toBeVisible();await page.keyboard.press('Escape');
 await page.evaluate(()=>{(window as any).failUsage=true});await page.getByRole('button',{name:'刷新',exact:true}).click();
 await expect(page.getByRole('alert')).toContainText('今日用量读取失败');await expect(page.getByRole('button',{name:'仪表盘布局调整',exact:true})).toBeVisible();
 await expect(page.getByTestId('dashboard-input')).toHaveText('暂无数据');
 await page.evaluate(()=>{(window as any).failUsage=false;(window as any).emptyUsage=true});await page.getByRole('button',{name:'重试',exact:true}).click();
 await expect(page.getByRole('alert')).toHaveCount(0);await expect(page.getByTestId('dashboard-input')).toHaveText('暂无数据');await expect(page.locator('.daily-cache')).not.toContainText('0%');
 expect(errors).toEqual([]);
});

test('appearance autosaves across navigation, defaults to first blue, and preserves the app icon',async({page})=>{
 await desktop(page);const icon=await page.locator('.titlebar-drag img').getAttribute('src');
 await expect(page.locator('.current-provider .active-tag')).toHaveCount(0);
 const blueFilter=await page.locator('.provider-badge').evaluate(el=>getComputedStyle(el).filter);
 expect(blueFilter).not.toBe('none');
 await page.getByRole('button',{name:'设置',exact:true}).click();
 await expect(page.getByRole('button',{name:'湖蓝主题色',exact:true})).toHaveAttribute('aria-pressed','true');
 await expect(page.getByRole('button',{name:'保存设置',exact:true})).toHaveCount(0);
 await page.getByRole('button',{name:'深色',exact:true}).click();await page.getByRole('button',{name:'绿色主题色',exact:true}).click();
 await page.getByLabel('配色风格').selectOption('soft');
 await page.getByRole('button',{name:'仪表盘',exact:true}).click();
 await expect.poll(()=>page.evaluate(()=>(window as any).calls.filter((c:any)=>c.command==='save_settings').at(-1)?.args.settings)).toMatchObject({theme:'dark',accentColor:'#26a954',colorStyle:'soft'});
 await expect(page.locator('html')).toHaveAttribute('data-theme','dark');
 await expect(page.locator('.titlebar-drag img')).toHaveAttribute('src',icon!);
 const greenFilter=await page.locator('.provider-badge').evaluate(el=>getComputedStyle(el).filter);
 expect(greenFilter).not.toBe(blueFilter);await expect(page.locator('.provider-wash')).toHaveCSS('filter',greenFilter);
 await page.getByRole('button',{name:'设置',exact:true}).click();await expect(page.getByRole('button',{name:'绿色主题色',exact:true})).toHaveAttribute('aria-pressed','true');
 await page.getByRole('button',{name:'重置主题色',exact:true}).click();await expect(page.getByTestId('settings-save-state')).toHaveText('已自动保存');
 expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.command==='save_settings').at(-1).args.settings.accentColor)).toBe('#007acc');
});

test('autosave serializes slow writes, keeps newer edits, and retries failures without losing input',async({page})=>{
 await desktop(page);
 await page.evaluate(()=>{
  const w=window as any,invoke=w.__TAURI_INTERNALS__.invoke;
  w.startedSettings=[];w.holdNextSave=true;w.failNextSave=false;w.concurrentSaves=0;w.maxConcurrentSaves=0;
  w.__TAURI_INTERNALS__.invoke=async(command:string,args:any)=>{
   if(command!=='save_settings')return invoke(command,args);
   w.startedSettings.push(structuredClone(args.settings));w.concurrentSaves++;w.maxConcurrentSaves=Math.max(w.maxConcurrentSaves,w.concurrentSaves);
   try{
    if(w.holdNextSave){w.holdNextSave=false;await new Promise(resolve=>{w.finishSettings=resolve})}
    if(w.failNextSave){w.failNextSave=false;throw new Error('配置文件暂时不可写')}
    return await invoke(command,args);
   }finally{w.concurrentSaves--}
  };
 });
 await page.getByRole('button',{name:'设置',exact:true}).click();await page.getByRole('button',{name:'深色',exact:true}).click();
 await expect.poll(()=>page.evaluate(()=>(window as any).startedSettings.length)).toBe(1);
 await page.getByRole('button',{name:'绿色主题色',exact:true}).click();await page.getByLabel('Codex 数据目录',{exact:false}).first().fill('C:\\NewCodex');
 await page.getByRole('button',{name:'仪表盘',exact:true}).click();await page.evaluate(()=>(window as any).finishSettings());
 await expect.poll(()=>page.evaluate(()=>(window as any).calls.filter((c:any)=>c.command==='save_settings').at(-1)?.args.settings)).toMatchObject({theme:'dark',accentColor:'#26a954',codexHome:'C:\\NewCodex'});
 expect(await page.evaluate(()=>(window as any).maxConcurrentSaves)).toBe(1);
 await page.getByRole('button',{name:'设置',exact:true}).click();await page.evaluate(()=>{(window as any).failNextSave=true});
 await page.getByRole('button',{name:'紫色主题色',exact:true}).click();
 await expect(page.getByRole('alert')).toContainText('配置文件暂时不可写');await expect(page.getByTestId('settings-save-state')).toHaveText('未保存');
 await expect(page.getByRole('button',{name:'紫色主题色',exact:true})).toHaveAttribute('aria-pressed','true');
 await page.getByRole('button',{name:'重试保存',exact:true}).click();await expect(page.getByTestId('settings-save-state')).toHaveText('已自动保存');
 await expect(page.getByRole('alert')).toHaveCount(0);
 expect(await page.evaluate(()=>(window as any).calls.filter((c:any)=>c.command==='save_settings').at(-1).args.settings)).toMatchObject({theme:'dark',accentColor:'#a53ee7',codexHome:'C:\\NewCodex'});
});

test('dashboard and editor fit desktop and minimum window, with dark appearance and no console errors',async({page})=>{
 const errors:string[]=[];page.on('pageerror',e=>errors.push(e.message));await page.setViewportSize({width:1440,height:1000});await desktop(page);
 await expect(page.locator('.daily-summary')).toBeInViewport();expect(await page.locator('.daily-summary').evaluate(el=>el.getBoundingClientRect().bottom)).toBeLessThanOrEqual(1000);await page.screenshot({path:join(tmpdir(),'easy-switch-dashboard-light.png')});
 await page.setViewportSize({width:940,height:620});await page.screenshot({path:join(tmpdir(),'easy-switch-dashboard-compact.png')});
 expect(await page.locator('main').evaluate(el=>el.scrollWidth<=el.clientWidth)).toBe(true);
 await page.getByRole('button',{name:'管理模型',exact:true}).click();await expect(page.getByRole('dialog').locator('.modal-footer')).toBeInViewport();
 expect(await page.getByRole('dialog').evaluate(el=>el.scrollWidth<=el.clientWidth)).toBe(true);await page.screenshot({path:join(tmpdir(),'easy-switch-editor-compact-v3.png')});await page.keyboard.press('Escape');
 await page.getByRole('button',{name:'设置',exact:true}).click();await page.getByRole('button',{name:'深色',exact:true}).click();await expect(page.getByTestId('settings-save-state')).toHaveText('已自动保存');
 await page.getByRole('button',{name:'仪表盘',exact:true}).click();await page.setViewportSize({width:1440,height:1000});await page.screenshot({path:join(tmpdir(),'easy-switch-dashboard-dark.png')});
 await expect(page.locator('.usage-input').last()).toHaveCSS('background-color',await page.getByRole('button',{name:'管理供应商',exact:true}).evaluate(el=>getComputedStyle(el).backgroundColor));
 expect(errors).toEqual([]);
});

test('custom black, white and yellow colors retain readable action labels in both themes',async({page})=>{
 await desktop(page);await page.getByRole('button',{name:'设置',exact:true}).click();
 for(const theme of ['浅色','深色']){
  await page.getByRole('button',{name:theme,exact:true}).click();
  for(const color of ['#000000','#ffffff','#ffff00']){
   await page.getByLabel('自定义主题色').fill(color);
   await expect.poll(()=>page.getByRole('button',{name:theme,exact:true}).evaluate(el=>{
    const style=getComputedStyle(el);
    const luminance=(color:string)=>color.match(/[\d.]+/g)!.slice(0,3).map(Number).map(n=>{const c=n/255;return c<=.04045?c/12.92:((c+.055)/1.055)**2.4}).reduce((s,n,i)=>s+n*[.2126,.7152,.0722][i],0);
    const a=luminance(style.color),b=luminance(style.backgroundColor);return (Math.max(a,b)+.05)/(Math.min(a,b)+.05);
   })).toBeGreaterThanOrEqual(4.5);
  }
 }
});

test('session read failures expose retry and recover the conversation without changing selection',async({page})=>{
 await desktop(page);
 await page.evaluate(()=>{
  const w=window as any,invoke=w.__TAURI_INTERNALS__.invoke;w.failDetail=true;w.failScan=true;
  w.__TAURI_INTERNALS__.invoke=async(command:string,args:any)=>{if(command==='scan_sessions'&&w.failScan)throw new Error('扫描失败');if(command==='session_detail'&&w.failDetail)throw new Error('读取失败');return invoke(command,args)};
 });
 await page.getByRole('button',{name:'会话',exact:true}).click();await expect(page.getByText('会话读取失败：Error: 扫描失败')).toBeVisible();
 await page.evaluate(()=>{(window as any).failScan=false});await page.getByRole('button',{name:'重试',exact:true}).click();await expect(page.locator('.session-row')).toHaveCount(4);
 await page.locator('.session-row button').filter({hasText:'仪表盘布局调整'}).click();await expect(page.getByText('无法读取会话',{exact:true})).toBeVisible();
 await page.evaluate(()=>{(window as any).failDetail=false});await page.getByRole('button',{name:'重试读取',exact:true}).click();await expect(page.locator('.message pre')).toHaveText('session-0');
});


test('Weekly chart fits large values and keeps tiny output proportional',async({page})=>{
 await desktop(page);
 await page.evaluate(()=>{
  const w=window as any,invoke=w.__TAURI_INTERNALS__.invoke;
  w.__TAURI_INTERNALS__.invoke=async(command:string,args:any)=>{
   if(command!=='usage_report'||args.query.bucketStarts.length!==7)return invoke(command,args);
   const values=[44730000,121130000,180120000,0,0,110000000,70260000];
   const buckets=args.query.bucketStarts.map((start:number,i:number)=>({start,input:values[i],output:i===2?1000:i===5?11000000:0,cached:0}));
   return {input:values.reduce((a,b)=>a+b,0),output:11001000,cached:0,buckets,models:[],sessions:[],availableModels:[],scannedFiles:4,warnings:[]};
  };
 });
 await page.getByRole('button',{name:'刷新',exact:true}).click();
 const chart=page.locator('.dashboard-trend'),columns=chart.locator('.usage-column');
 await expect(columns.nth(2)).toHaveAttribute('title',/输入 180,120,000，输出 1,000 Token/);
 const tiny=await columns.nth(2).locator('.usage-output').evaluate(el=>el.getBoundingClientRect().height);
 expect(tiny).toBeLessThan(1);
 expect(await columns.nth(5).locator('.usage-output').evaluate(el=>el.getBoundingClientRect().height)).toBeGreaterThan(5);
 for(const width of [1180,1000,940]){
  await page.setViewportSize({width,height:840});
  await chart.scrollIntoViewIfNeeded();
  const bounds=await chart.boundingBox();
  const labels=chart.locator('.usage-stack small,.usage-grid span,.usage-tick');
  for(const label of await labels.all()){
   const box=await label.boundingBox();expect(box).not.toBeNull();
   expect(box!.x).toBeGreaterThanOrEqual(bounds!.x+8);expect(box!.x+box!.width).toBeLessThanOrEqual(bounds!.x+bounds!.width-8);
   expect(box!.y).toBeGreaterThan(bounds!.y);expect(box!.y+box!.height).toBeLessThan(bounds!.y+bounds!.height);
  }
  const numbers=await chart.locator('.usage-stack small').all();
  for(let i=1;i<numbers.length;i++){const a=await numbers[i-1].boundingBox(),b=await numbers[i].boundingBox();expect(a!.x+a!.width).toBeLessThan(b!.x)}
  await chart.screenshot({path:`artifacts/screenshots/chart-large-${width}.png`});
 }
});


test('Theme surfaces follow every accent and keep text readable in light and dark modes',async({page},testInfo)=>{
 test.setTimeout(60000);
 await page.setViewportSize({width:1180,height:840});await desktop(page);
 await page.getByRole('button',{name:'设置',exact:true}).click();
 const samples:any[]=[];
 for(const theme of ['浅色','深色']){
  await page.getByRole('button',{name:theme,exact:true}).click();
  const backgrounds=new Set<string>();
  for(let index=0;index<11;index++){
   if(index<9)await page.locator('.color-swatch').nth(index).click();
   else await page.locator('.custom-color input').evaluate((el:HTMLInputElement,color)=>{Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value')!.set!.call(el,color);el.dispatchEvent(new Event('input',{bubbles:true}));el.dispatchEvent(new Event('change',{bubbles:true}))},index===9?'#000000':'#ffffff');
   await expect(page.getByTestId('settings-save-state')).toHaveText('已自动保存');
   const sample=await page.evaluate(()=>{
    const probe=document.createElement('span');probe.hidden=true;document.body.append(probe);
    const canvas=document.createElement('canvas');canvas.width=canvas.height=1;const context=canvas.getContext('2d')!;
    const rgb=(token:string)=>{probe.style.color=`var(--${token})`;context.fillStyle=getComputedStyle(probe).color;context.fillRect(0,0,1,1);return Array.from(context.getImageData(0,0,1,1).data).slice(0,3)};
    const colors=Object.fromEntries(['bg','surface','sidebar','soft','text','muted','accent','on-accent'].map(key=>[key,rgb(key)]));probe.remove();return colors;
   });
   const luminance=(rgb:number[])=>rgb.map(v=>{const c=v/255;return c<=.04045?c/12.92:((c+.055)/1.055)**2.4}).reduce((sum,v,i)=>sum+v*[.2126,.7152,.0722][i],0);
   const contrast=(a:number[],b:number[])=>{const x=luminance(a),y=luminance(b);return (Math.max(x,y)+.05)/(Math.min(x,y)+.05)};
   for(const background of ['bg','surface','sidebar','soft'])for(const foreground of ['text','muted'])expect.soft(contrast(sample[background],sample[foreground]),`${theme} ${index} ${background}/${foreground}`).toBeGreaterThanOrEqual(4.5);
   expect.soft(contrast(sample.accent,sample['on-accent'])).toBeGreaterThanOrEqual(4.5);
   backgrounds.add(JSON.stringify(sample.sidebar));samples.push({theme,index,...sample});
  }
  expect(backgrounds.size).toBe(11);
  for(const color of ['湖蓝','绿色']){
   await page.getByRole('button',{name:color+'主题色',exact:true}).click();await expect(page.getByTestId('settings-save-state')).toHaveText('已自动保存');
   await page.getByRole('button',{name:'仪表盘',exact:true}).click();await expect(page.getByTestId('dashboard-input')).toHaveText('128.4K');
   const actual=await page.evaluate(()=>Object.fromEntries(['.window-titlebar','.sidebar','.recent-sessions thead','.daily-cache>div'].map(selector=>[selector,getComputedStyle(document.querySelector(selector)!).backgroundColor])));
   expect(actual['.window-titlebar']).toBe(actual['.sidebar']);expect(actual['.recent-sessions thead']).toBe(actual['.daily-cache>div']);
   await page.screenshot({path:`artifacts/screenshots/theme-${theme==='浅色'?'light':'dark'}-${color==='湖蓝'?'blue':'green'}.png`,animations:'disabled'});
   await page.getByRole('button',{name:'设置',exact:true}).click();
  }
 }
 await testInfo.attach('theme-colors',{body:JSON.stringify(samples),contentType:'application/json'});
});
