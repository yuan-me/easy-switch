// Synthetic native IPC benchmark. Never reads or changes the real Codex home.
const {chromium}=require('@playwright/test');
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {spawn}=require('node:child_process');
const {randomUUID}=require('node:crypto');
(async()=>{
 const label=process.argv[2]??'current',root=path.resolve('artifacts/performance-fixture'),home=path.join(root,'home'),store=path.join(root,'store');
 fs.mkdirSync(path.join(home,'sessions'),{recursive:true});fs.mkdirSync(store,{recursive:true});
 const idsFile=path.join(root,'ids.json');let ids;
 if(fs.existsSync(idsFile))ids=JSON.parse(fs.readFileSync(idsFile));else{
  ids=Array.from({length:500},()=>randomUUID());
  const body=Array.from({length:40},()=>JSON.stringify({type:'response_item',payload:{type:'message',role:'assistant',content:[{type:'output_text',text:'synthetic '.repeat(250)}]}})).join('\n');
  ids.forEach(id=>fs.writeFileSync(path.join(home,'sessions',id+'.jsonl'),JSON.stringify({type:'session_meta',payload:{id,cwd:home,model_provider:'openai'}})+'\n'+body+'\n'));
  fs.writeFileSync(idsFile,JSON.stringify(ids));
 }
 fs.writeFileSync(path.join(store,'settings.json'),JSON.stringify({codexHome:home,automaticUpdates:false,automaticDownload:false,theme:'light'}));
 const started=performance.now(),proc=spawn(path.resolve('target/release/easy-switch.exe'),['--store',store],{env:{...process.env,WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:'--remote-debugging-port=47846 --remote-debugging-address=127.0.0.1'},windowsHide:true,stdio:'ignore'});let browser;
 try{
  for(let i=0;i<100;i++){try{browser=await chromium.connectOverCDP('http://127.0.0.1:47846');break}catch{await new Promise(r=>setTimeout(r,100))}}
  assert.ok(browser);const page=browser.contexts()[0].pages()[0];await page.waitForSelector('h1');
  const invoke=(command,args={})=>page.evaluate(({command,args})=>window.__TAURI_INTERNALS__.invoke(command,args),{command,args});
  const boot=await invoke('bootstrap');assert.equal(boot.settings.codexHome,home);
  const startupMs=performance.now()-started,scanStarted=performance.now(),scan=await invoke('scan_sessions');assert.equal(scan.sessions.length,500);assert.equal(scan.warnings.length,0);
  const scanMs=performance.now()-scanStarted,details=[];
  for(const id of ids.slice(0,7)){const t=performance.now(),detail=await invoke('session_detail',{id});assert.equal(detail.messages.length,40);details.push(performance.now()-t)}
  const median=[...details].sort((a,b)=>a-b)[3];
  const report={label,synthetic:true,sessions:500,eventsPerSession:40,startupMs,scanMs,detailMs:details,detailMedianMs:median};
  fs.writeFileSync(path.resolve('artifacts/performance-'+label+'.json'),JSON.stringify(report,null,2));console.log(JSON.stringify(report));
 }finally{if(browser)await browser.close();proc.kill()}
})().catch(e=>{console.error(e.message);process.exitCode=1});
