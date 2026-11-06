import {test,expect} from '@playwright/test';

for(const scenario of ['start-failed','fast-failed','stop-failed','success'] as const){
 test(`provider state follows saved configuration after ${scenario}`,async({page})=>{
  await page.addInitScript(scenario=>{
   const api={id:'api',name:'测试供应商',mode:'Api',protocol:'Responses',baseUrl:'https://example.com/v1',model:'test-model',members:[],headers:{},hasKey:true};
   const state={providers:[{...api,id:'official',name:'OpenAI 官方',mode:'Official',model:'',baseUrl:''},api],settings:{theme:'light',activeProviderId:'official',scrollPositions:{},automaticUpdates:false,automaticDownload:false},version:'test',migrationError:null};
   Object.assign(window,{isTauri:true,__TAURI_EVENT_PLUGIN_INTERNALS__:{unregisterListener:()=>{}},__TAURI_INTERNALS__:{
    metadata:{currentWindow:{label:'main'},currentWebview:{label:'main'}},transformCallback:()=>1,
    invoke:async(command:string,args:any)=>{
     if(command==='bootstrap')return structuredClone(state);
     if(command==='interrupted_operations')return [];
     if(command==='scan_sessions')return {sessions:[],warnings:[]};
     if(command==='usage_report')return {input:0,output:0,cached:0,buckets:[],models:[],sessions:[],warnings:[],scannedFiles:0};
     if(command==='switch_provider'){
      if(scenario==='stop-failed')throw new Error('Codex 未完全退出，未修改数据');
      state.settings.activeProviderId=args.id;
      if(scenario==='start-failed')throw new Error('配置已保存。未能启动 Codex，请手动启动');
      if(scenario==='fast-failed')throw new Error('配置已保存。Codex 已启动，但 Fast 解锁未成功：Fast 模块未确认生效');
      return 'Codex 已启动';
     }
     return false;
    }
   }});
  },scenario);
  await page.goto('/');
  await expect(page.locator('.current-provider h2')).toHaveText('OpenAI 官方');
  await page.getByRole('button',{name:'供应商',exact:true}).click();
  const api=page.locator('.provider-card').filter({has:page.getByRole('heading',{name:'测试供应商',exact:true})});
  await api.getByRole('button',{name:'切换',exact:true}).click();
  await page.getByRole('dialog').getByRole('button',{name:'切换并重启',exact:true}).click();
  const selected=scenario==='stop-failed'?'OpenAI 官方':'测试供应商';
  await expect(page.locator('.provider-card.enabled h2')).toContainText(selected);
  await expect(page.locator('.provider-card.enabled')).toHaveCount(1);
  await expect(page.locator('.provider-card.enabled').getByRole('button',{name:'重新应用',exact:true})).toBeEnabled();
  if(scenario==='success')await expect(page.getByRole('status')).toContainText('切换成功');
  else{
   await expect(page.getByRole('alert')).toContainText(scenario==='start-failed'?'未能启动 Codex':scenario==='fast-failed'?'Codex 已启动，但 Fast 解锁未成功':'Codex 未完全退出');
   await expect(page.getByText('切换成功',{exact:true})).toHaveCount(0);
  }
  await expect(page.locator('.operation-progress')).toHaveCount(0);
  await page.getByRole('button',{name:'仪表盘',exact:true}).click();
  await expect(page.locator('.current-provider h2')).toHaveText(selected);
 });
}
