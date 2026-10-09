import {test,expect} from '@playwright/test';
test('usage periods, model filtering, session navigation and timed notifications',async({page})=>{
 await page.clock.install({time:new Date('2026-10-03T12:00:00+08:00')});
 await page.addInitScript(()=>{
  const sessions=Array.from({length:12},(_,i)=>({id:'session-'+i,title:i?'合成会话 '+i:'界面优化讨论',cwd:'D:\\projects\\easy-switch',provider:'openai',path:'',updated:1790985600,archived:false,database:null}));
  Object.assign(window,{isTauri:true,__TAURI_EVENT_PLUGIN_INTERNALS__:{unregisterListener:()=>{}},__TAURI_INTERNALS__:{
   metadata:{currentWindow:{label:'main'},currentWebview:{label:'main'}},transformCallback:()=>1,
   invoke:async(command:string,args:any)=>{
    if(command==='bootstrap')return {providers:[],settings:{theme:'light',scrollPositions:{}},version:'1.1.1',migrationError:null};
    if(command==='scan_sessions')return {sessions,warnings:[]};
    if(command==='session_detail')return {messages:Array.from({length:12},(_,i)=>({role:i%2?'assistant':'user',text:'这是合成会话内容，用于验证紧凑布局与阅读位置。'.repeat(3),time:'2026-10-03T12:00:00+08:00'})),tokens:[],relations:[],hasEncryptedContent:false,truncated:false};
    if(command==='usage_report'){
     const q=args.query,factor=q.model?0.5:1,input=900000*factor,output=300000*factor,cached=540000*factor;
     return {input,output,cached,buckets:q.bucketStarts.map((start:number,i:number)=>({start,input:i===0?input:0,output:i===0?output:0,cached:i===0?cached:0})),models:[{id:'A',name:'模型 A',input,output,cached}],sessions:[{id:'session-0',name:'界面优化讨论',input,output,cached}],availableModels:['模型 A','模型 B'],warnings:[],scannedFiles:12};
    }
    if(command==='plugin:window|is_maximized')return false;
    return 1;
   }
  }});
  Object.defineProperty(navigator,'clipboard',{value:{writeText:async()=>{}}});
 });
 const errors:string[]=[];page.on('pageerror',e=>errors.push(e.message));
 await page.goto('/');await page.getByRole('button',{name:'Token 统计',exact:true}).click();
 await expect(page.getByTestId('usage-total')).toHaveText('1.2M');await expect(page.locator('.usage-column')).toHaveCount(7);
 await page.getByRole('button',{name:'日',exact:true}).click();await expect(page.locator('.usage-column')).toHaveCount(24);
 await page.getByRole('button',{name:'月',exact:true}).click();await expect(page.locator('.usage-column')).toHaveCount(31);
 await page.getByLabel('统计模型').selectOption('模型 A');await expect(page.getByTestId('usage-total')).toHaveText('600K');
 await page.setViewportSize({width:940,height:620});
 expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
 await page.getByRole('button',{name:'界面优化讨论',exact:true}).click();
 await expect(page.locator('.session-row')).toHaveCount(12);await expect(page.locator('.detail-heading h2')).toHaveText('界面优化讨论');
 await expect(page.locator('.detail-footer')).toBeInViewport();
 await expect(page.locator('.message pre').first()).toHaveCSS('user-select','none');
 await page.getByRole('button',{name:'复制线程 ID',exact:true}).click();await expect(page.getByRole('status')).toContainText('线程 ID 已复制');
 await page.clock.fastForward(9000);await expect(page.getByRole('status')).toBeVisible();
 await page.getByRole('button',{name:'复制线程 ID',exact:true}).click();
 await page.clock.fastForward(2000);await expect(page.getByRole('status')).toBeVisible();
 await page.clock.fastForward(8000);await expect(page.locator('.notice')).toHaveCount(0);
 await page.getByRole('button',{name:'复制线程 ID',exact:true}).click();await page.getByRole('button',{name:'关闭提示',exact:true}).click();await expect(page.locator('.notice')).toHaveCount(0);
 expect(errors).toEqual([]);
});
