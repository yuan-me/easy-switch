// Native WebView2 animation sample with an isolated store and synthetic providers.
const {chromium}=require('@playwright/test');
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {spawn,execFileSync}=require('node:child_process');
const wait=ms=>new Promise(r=>setTimeout(r,ms));
(async()=>{
 const label=process.argv[2]||'current',out=path.resolve('artifacts/animation-'+label);
 assert.equal(execFileSync('powershell.exe',['-NoProfile','-Command','@(Get-Process easy-switch -ErrorAction SilentlyContinue).Count'],{windowsHide:true,encoding:'utf8'}).trim(),'0','Close Easy Switch before this isolated test');
 const home=path.join(out,'home'),store=path.join(out,'store');
 fs.mkdirSync(home,{recursive:true});fs.mkdirSync(store,{recursive:true});
 fs.mkdirSync(path.join(home,'sessions'),{recursive:true});
 fs.writeFileSync(path.join(home,'sessions','synthetic.jsonl'),[
  {type:'session_meta',payload:{id:'11111111-1111-4111-8111-111111111111',cwd:home,model_provider:'openai'}},
  ...Array.from({length:500},()=>({timestamp:'2026-10-12T00:00:00Z',type:'response_item',payload:{type:'message',role:'assistant',content:[{type:'output_text',text:'synthetic conversation '.repeat(50)}]}}))
 ].map(v=>JSON.stringify(v)).join('\n')+'\n');
 fs.writeFileSync(path.join(store,'settings.json'),JSON.stringify({codexHome:home,automaticUpdates:false,automaticDownload:false,theme:'light',activeProviderId:'synthetic'}));
 const models=Array.from({length:28},(_,i)=>'synthetic-model-'+String(i).padStart(2,'0'));
 fs.writeFileSync(path.join(store,'providers.json'),JSON.stringify([{id:'synthetic',name:'动画验收供应商',mode:'Api',protocol:'Responses',baseUrl:'https://example.invalid/v1',model:models[0]}]));
 fs.writeFileSync(path.join(store,'model-catalogs.json'),JSON.stringify({providers:{synthetic:{available:models,excluded:[],added:[models[1]],removed:[],checkedAt:Math.floor(Date.now()/1000)}}}));
 fs.writeFileSync(path.join(home,'config.toml'),"model_provider = 'synthetic'\nmodel = 'synthetic-model-00'\n");
 const started=performance.now();
 const proc=spawn(path.resolve('target/release/easy-switch.exe'),['--store',store],{env:{...process.env,WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:'--remote-debugging-port=47849 --remote-debugging-address=127.0.0.1'},windowsHide:true,stdio:'ignore'});
 let browser;
 try{
  for(let i=0;i<100;i++){try{browser=await chromium.connectOverCDP('http://127.0.0.1:47849');break}catch{await wait(100)}}
  assert.ok(browser,'Native WebView2 did not start');const page=browser.contexts()[0].pages()[0];
  await page.waitForSelector('h1');
  const invoke=(command,args={})=>page.evaluate(({command,args})=>window.__TAURI_INTERNALS__.invoke(command,args),{command,args});
  assert.equal((await invoke('bootstrap')).settings.codexHome,home);
  const startupMs=performance.now()-started;
  const nativeHandle=Number(execFileSync('powershell.exe',['-NoProfile','-Command',`(Get-Process -Id ${proc.pid}).MainWindowHandle.ToInt64()`],{windowsHide:true,encoding:'utf8'}).trim());
  assert.ok(nativeHandle>0,'The styled native window must become visible');
  await page.getByRole('button',{name:'会话',exact:true}).click();
  await page.locator('.session-row button').first().click();
  await page.waitForFunction(()=>document.querySelectorAll('.message').length===500);
  await page.getByRole('button',{name:'供应商',exact:true}).click();
  await page.getByRole('button',{name:'编辑',exact:true}).click();assert.equal(await page.locator('.model-choice').count(),28);await page.keyboard.press('Escape');
  const cdp=await page.context().newCDPSession(page);await cdp.send('Performance.enable');
  const samples=[];
  for(let i=0;i<9;i++){
   const before=(await cdp.send('Performance.getMetrics')).metrics;
   const frames=await page.evaluate(async()=>{
    const frames=[],start=performance.now();let last=start;
    [...document.querySelectorAll('button')].find(el=>el.textContent==='编辑').click();
    await new Promise(resolve=>{function frame(now){frames.push(now-last);last=now;if(now-start<300)requestAnimationFrame(frame);else resolve()}requestAnimationFrame(frame)});
    return frames;
   });
   const after=(await cdp.send('Performance.getMetrics')).metrics;
   const cost=name=>1000*(after.find(x=>x.name===name).value-before.find(x=>x.name===name).value);
   samples.push({frames,taskMs:cost('TaskDuration'),layoutMs:cost('LayoutDuration'),styleMs:cost('RecalcStyleDuration')});
   if(i===8)await page.screenshot({path:path.join(out,'editor.png')});
   await page.keyboard.press('Escape');await wait(100);
  }
  await page.getByRole('button',{name:'编辑',exact:true}).click();
  await page.getByRole('dialog').waitFor();
  const css=await page.evaluate(()=>{const dialog=document.querySelector('dialog');return {backdropFilter:getComputedStyle(dialog,'::backdrop').backdropFilter,transition:getComputedStyle(dialog).transitionProperty,duration:getComputedStyle(dialog).transitionDuration,opacity:getComputedStyle(dialog).opacity}});
  assert.equal(await page.evaluate(()=>CSS.supports('appearance','base-select')),true,'WebView2 must support themed native selects');
  const protocol=page.getByLabel('协议');await protocol.click();
  assert.equal(await protocol.evaluate(el=>el.matches(':open')),true);
  assert.equal(await protocol.evaluate(el=>getComputedStyle(el,'::picker(select)').backgroundColor),'rgb(255, 255, 255)');
  await page.screenshot({path:path.join(out,'native-dropdown.png')});
  await page.keyboard.press('Escape');assert.equal(await page.getByRole('dialog').isVisible(),true);
  const dialog=page.getByRole('dialog');
  await dialog.evaluate(el=>Promise.all(el.getAnimations().map(animation=>animation.finished)));
  const layout=await dialog.evaluate(el=>{const box=el.getBoundingClientRect();return {width:box.width,centerOffset:Math.abs(box.x+box.width/2-innerWidth/2),columns:getComputedStyle(el.querySelector('.model-choices')).gridTemplateColumns.split(' ').length}});
  assert.ok(layout.width>800);assert.ok(layout.centerOffset<1);assert.equal(layout.columns,2);
  await assert.rejects(invoke('reveal_provider_key',{id:'synthetic'}),/无法读取已保存的 API Key/);
  await dialog.getByLabel('API Key',{exact:true}).fill('synthetic-test-key');
  await dialog.getByLabel('选择默认模型').selectOption(models[1]);
  await dialog.getByLabel('加入 '+models[0],{exact:true}).uncheck();
  await dialog.getByRole('button',{name:'全选',exact:true}).click();
  assert.equal(await dialog.locator('.model-choice input:checked').count(),28);
  assert.equal(await dialog.getByRole('button',{name:'取消全选',exact:true}).isEnabled(),true);
  await dialog.getByRole('button',{name:'取消全选',exact:true}).click();
  assert.equal(await dialog.locator('.model-choice input:checked').count(),1);
  assert.equal(await dialog.getByLabel('加入 '+models[1],{exact:true}).isChecked(),true);
  await dialog.getByRole('button',{name:'全选',exact:true}).click();
  await dialog.getByLabel('加入 '+models[0],{exact:true}).uncheck();
  await dialog.locator('.form-scroll').evaluate(el=>{const row=el.querySelector('.model-refresh').parentElement;el.scrollTop+=row.getBoundingClientRect().top-el.getBoundingClientRect().top-16});
  await page.screenshot({path:path.join(out,'unified-model-editor.png')});
  await dialog.getByRole('button',{name:'保存配置'}).click();await dialog.waitFor({state:'hidden'});
  const saved=await invoke('bootstrap');assert.equal(saved.providers[0].model,models[1]);assert.deepEqual(saved.modelCatalogs.synthetic.excluded,[models[0]]);
  assert.match(fs.readFileSync(path.join(home,'config.toml'),'utf8'),/model = ["']synthetic-model-01["']/);
  const catalog=JSON.parse(fs.readFileSync(path.join(home,'easy-switch-models.json'),'utf8'));
  assert.ok(!catalog.models.some(item=>item.slug===models[0]));assert.equal(await page.locator('.model-notice').count(),0);
  assert.ok(!JSON.stringify(saved).includes('synthetic-test-key'));assert.ok(!saved.providers[0].protectedKey);
  const protectedBefore=fs.readFileSync(path.join(store,'providers.json'),'utf8');assert.ok(!protectedBefore.includes('synthetic-test-key'));
  await page.getByRole('button',{name:'编辑',exact:true}).click();
  const secret=page.getByLabel('API Key',{exact:true});assert.equal(await secret.inputValue(),'');
  await page.getByRole('button',{name:'显示 API Key',exact:true}).click();
  await page.waitForFunction(()=>document.querySelector('.api-key-input input').type==='text');
  assert.equal(await secret.inputValue(),'synthetic-test-key');
  await page.getByRole('button',{name:'隐藏 API Key',exact:true}).click();assert.equal(await secret.getAttribute('type'),'password');
  await page.keyboard.press('Escape');assert.equal(fs.readFileSync(path.join(store,'providers.json'),'utf8'),protectedBefore);
  await assert.rejects(invoke('reveal_provider_key',{id:'missing'}),/无法读取已保存的 API Key/);
  const steady=samples.slice(1),median=values=>values.sort((a,b)=>a-b)[Math.floor(values.length/2)];
  const report={label,synthetic:true,webview:await page.evaluate(()=>navigator.userAgent),customSelectSupported:true,defaultAndChoicesSaved:true,selectAllVerified:true,keyRevealVerified:true,layout,visibleAfterReady:true,retainedMessages:500,editorModels:28,startupMs,css,viewport:await page.evaluate(()=>({width:innerWidth,height:innerHeight,dpr:devicePixelRatio})),medianTaskMs:median(steady.map(s=>s.taskMs)),medianLayoutMs:median(steady.map(s=>s.layoutMs)),medianStyleMs:median(steady.map(s=>s.styleMs)),framesOver25ms:steady.flatMap(s=>s.frames).filter(ms=>ms>25).length,samples,realCodexTouched:false};
  fs.writeFileSync(path.join(out,'result.json'),JSON.stringify(report,null,2));
  console.log(JSON.stringify({...report,samples:undefined}));
 }finally{if(browser)await browser.close();proc.kill();await new Promise(resolve=>proc.exitCode!==null?resolve():proc.once('exit',resolve))}
})().catch(e=>{console.error(e);process.exitCode=1});
