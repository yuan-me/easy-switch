// Native/VM acceptance against isolated synthetic files; never opens real Codex data.
// Usage: node scripts/usage-smoke.cjs [exe] [output-directory]
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {spawn,execFileSync}=require('node:child_process'),{randomUUID,createHash}=require('node:crypto');
let chromium;try{({chromium}=require('@playwright/test'))}catch{({chromium}=require('playwright-core'))}
const {DatabaseSync}=require('node:sqlite');
const wait=ms=>new Promise(r=>setTimeout(r,ms));
(async()=>{
 const exe=path.resolve(process.argv[2]||'target/release/easy-switch.exe');
 const out=path.resolve(process.argv[3]||'artifacts/usage-native-'+Date.now());
 const root=path.join(out,'fixture-'+Date.now()),home=path.join(root,'home'),store=path.join(root,'store');
 fs.mkdirSync(path.join(home,'sessions'),{recursive:true});fs.mkdirSync(store,{recursive:true});
 const report={status:'running',version:'',checks:[],realCodexTouched:false,errors:[]};
 const save=()=>fs.writeFileSync(path.join(out,'result.json'),JSON.stringify(report,null,2));
 const monday=new Date();monday.setHours(0,0,0,0);monday.setDate(monday.getDate()-(monday.getDay()+6)%7);
 const days=[112500,135000,157500,180000,135000,180000,0];
 const weights=[.3,.25,.2,.15,.1];const names=['界面优化讨论','代码审查','数据整理','文档编写','问题排查'];
 const db=new DatabaseSync(path.join(home,'state_5.sqlite'));db.exec('CREATE TABLE threads(id TEXT PRIMARY KEY,title TEXT,cwd TEXT,model_provider TEXT,rollout_path TEXT,updated_at INTEGER,archived INTEGER)');
 let expectedInput=0;
 for(let i=0;i<12;i++){
  const id=randomUUID(),file=path.join(home,'sessions','rollout_'+id+'.jsonl'),model=i%2?'模型 B':'模型 A';
  const lines=[{type:'session_meta',payload:{id,cwd:'D:\\projects\\easy-switch',model_provider:'openai'}}];
  let input=0,output=0,cached=0;
  if(i<5)for(let d=0;d<7;d++){
   const date=new Date(monday);date.setDate(date.getDate()+d);date.setHours(9);
   if(date>new Date()||!days[d])continue;
   const delta=Math.round(days[d]*weights[i]);input+=delta;output+=delta/3;cached+=delta*.6;expectedInput+=delta;
   lines.push({timestamp:date.toISOString(),type:'turn_context',payload:{model,turn_id:id+'-'+d}});
   const usage={timestamp:date.toISOString(),type:'event_msg',payload:{type:'token_count',info:{total_token_usage:{input_tokens:input,output_tokens:output,cached_input_tokens:cached},last_token_usage:{input_tokens:delta,output_tokens:delta/3,cached_input_tokens:delta*.6}}}};
   lines.push(usage,usage);
  }
  for(let n=0;n<16;n++)lines.push({timestamp:new Date().toISOString(),type:'response_item',payload:{type:'message',role:n%2?'assistant':'user',content:[{type:n%2?'output_text':'input_text',text:n%2?'这是合成会话。保留正文清晰字号，减少消息间距，常用操作放在阅读区边缘。':'会话界面紧凑一些，尽量多展示列表和正文。'}]}});
  fs.writeFileSync(file,lines.map(v=>JSON.stringify(v)).join('\n')+'\n');
  db.prepare('INSERT INTO threads VALUES(?,?,?,?,?,?,?)').run(id,names[i]||'合成会话 '+(i+1),'D:\\projects\\easy-switch','openai',file,Math.floor(Date.now()/1000)-i*120,0);
 }db.close();
 fs.writeFileSync(path.join(store,'settings.json'),JSON.stringify({codexHome:home,automaticUpdates:false,automaticDownload:false,theme:'light'}));
 const hashes=()=>Object.fromEntries(fs.readdirSync(path.join(home,'sessions')).map(f=>[f,createHash('sha256').update(fs.readFileSync(path.join(home,'sessions',f))).digest('hex')]));
 const before=hashes(),port=47848;
 const proc=spawn(exe,['--store',store],{env:{...process.env,WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:'--remote-debugging-port='+port+' --remote-debugging-address=127.0.0.1'},windowsHide:true,stdio:'ignore'});let browser;
 const captureScript=path.join(out,'capture.ps1');
 fs.writeFileSync(captureScript,`param([int]$TargetId,[string]$Output,[int]$Width=0,[int]$Height=0,[long]$WindowId=0)
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public class UsageWindow {
 [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left,Top,Right,Bottom; }
 [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr value);
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out Rect r);
 [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
 [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h,int n);
 [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h,IntPtr a,int x,int y,int w,int z,uint f);
 [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
 [StructLayout(LayoutKind.Sequential)] public struct Point { public int X,Y; }
 [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h,ref Point p);
 [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h,int a,out int v,int s);
}
'@
[UsageWindow]::SetProcessDpiAwarenessContext([IntPtr]::new(-4))|Out-Null
$windowHandle=if($WindowId -ne 0){[IntPtr]::new($WindowId)}else{(Get-Process -Id $TargetId).MainWindowHandle}
if($windowHandle -eq 0){throw 'No native window'}
[UsageWindow]::ShowWindow($windowHandle,9)|Out-Null
$scale=[UsageWindow]::GetDpiForWindow($windowHandle)/96
if($Width -gt 0){[UsageWindow]::SetWindowPos($windowHandle,[IntPtr]::Zero,30,30,[int]($Width*$scale),[int]($Height*$scale),4)|Out-Null}
[UsageWindow]::SetForegroundWindow($windowHandle)|Out-Null
[UsageWindow]::SetWindowPos($windowHandle,[IntPtr]::new(-1),0,0,0,0,19)|Out-Null
Start-Sleep -Milliseconds 500
$rect=[UsageWindow+Rect]::new();[UsageWindow]::GetWindowRect($windowHandle,[ref]$rect)|Out-Null
$client=[UsageWindow+Point]::new();[UsageWindow]::ClientToScreen($windowHandle,[ref]$client)|Out-Null
$backdrop=0;$dark=0
$hr=[UsageWindow]::DwmGetWindowAttribute($windowHandle,38,[ref]$backdrop,4)
[UsageWindow]::DwmGetWindowAttribute($windowHandle,20,[ref]$dark,4)|Out-Null
$bitmap=[Drawing.Bitmap]::new($rect.Right-$rect.Left,$rect.Bottom-$rect.Top)
$graphics=[Drawing.Graphics]::FromImage($bitmap)
$graphics.CopyFromScreen($rect.Left,$rect.Top,0,0,$bitmap.Size)
$bitmap.Save($Output,[Drawing.Imaging.ImageFormat]::Png);$graphics.Dispose();$bitmap.Dispose()
[UsageWindow]::SetWindowPos($windowHandle,[IntPtr]::new(-2),0,0,0,0,19)|Out-Null
@{captionInset=$client.Y-$rect.Top;handle=$windowHandle.ToInt64();backdrop=$backdrop;dark=$dark;dpi=[UsageWindow]::GetDpiForWindow($windowHandle);hresult=$hr;width=$rect.Right-$rect.Left;height=$rect.Bottom-$rect.Top}|ConvertTo-Json -Compress
`);
 let nativeHandle=0;
 function capture(name,width=1280,height=900){const state=JSON.parse(execFileSync('powershell.exe',['-NoProfile','-ExecutionPolicy','Bypass','-File',captureScript,'-TargetId',String(proc.pid),'-Output',path.join(out,name+'.png'),'-Width',String(width),'-Height',String(height),'-WindowId',String(nativeHandle)],{encoding:'utf8',windowsHide:true}).trim());nativeHandle=state.handle;assert.equal(state.backdrop,2,'Mica missing on the original native window');assert.ok(state.captionInset<=12*state.dpi/96,'Duplicate native titlebar restored');return state}
 try{
  for(let n=0;n<100;n++){try{browser=await chromium.connectOverCDP('http://127.0.0.1:'+port);break}catch{await wait(300)}}
  assert.ok(browser,'Native WebView2 did not start');const page=browser.contexts()[0].pages()[0];page.on('pageerror',e=>report.errors.push(e.message));await page.waitForSelector('h1');
  const invoke=(command,args={})=>page.evaluate(({command,args})=>window.__TAURI_INTERNALS__.invoke(command,args),{command,args});
  const boot=await invoke('bootstrap');assert.equal(boot.version,'1.1.2');assert.equal(boot.settings.codexHome,home);report.version=boot.version;
  const starts=Array.from({length:7},(_,i)=>{const d=new Date(monday);d.setDate(d.getDate()+i);return d.getTime()/1000}),end=new Date(monday);end.setDate(end.getDate()+7);
  const data=await invoke('usage_report',{query:{bucketStarts:starts,end:end.getTime()/1000,model:null}});
  assert.equal(data.input,expectedInput);assert.equal(data.output,expectedInput/3);assert.equal(data.cached,expectedInput*.6);assert.equal(data.warnings.length,0);assert.equal(data.scannedFiles,12);
  assert.equal(data.sessions.reduce((n,s)=>n+s.input,0),expectedInput);assert.equal(data.models.reduce((n,m)=>n+m.input,0),expectedInput);
  const filtered=await invoke('usage_report',{query:{bucketStarts:starts,end:end.getTime()/1000,model:'模型 A'}});
  assert.equal(filtered.input,data.models.find(m=>m.name==='模型 A').input);
  report.checks.push('native usage IPC: exact input/output/cache totals','duplicate cumulative events ignored','model and conversation sums agree');
  await page.getByRole('button',{name:'Token 统计',exact:true}).click();await page.waitForSelector('.usage-column');await wait(300);
  await page.screenshot({path:path.join(out,'usage-webview.png')});report.light=capture('usage-light');
  assert.equal(report.light.backdrop,2,'Windows DWM Mica backdrop missing');
  await page.getByRole('button',{name:'月',exact:true}).click();await wait(300);assert.ok(await page.locator('.usage-column').count()>=28);
  await page.getByRole('button',{name:'日',exact:true}).click();await wait(300);assert.ok([23,24,25].includes(await page.locator('.usage-column').count()));
  await page.getByRole('button',{name:'周',exact:true}).click();await wait(300);
  await page.getByLabel('深色',{exact:true}).click();await wait(400);report.dark=capture('usage-dark');assert.equal(await page.locator('html').getAttribute('data-theme'),'dark');
  await page.getByLabel('浅色',{exact:true}).click();await wait(300);
  await page.getByRole('button',{name:'最大化',exact:true}).click();await wait(300);assert.equal(await invoke('plugin:window|is_maximized'),true);
  await page.getByRole('button',{name:'还原窗口',exact:true}).click();await wait(300);assert.equal(await invoke('plugin:window|is_maximized'),false);
  await page.locator('.titlebar-drag').dispatchEvent('mousedown',{button:0,detail:2});await wait(300);assert.equal(await invoke('plugin:window|is_maximized'),true);
  await page.getByRole('button',{name:'还原窗口',exact:true}).click();await wait(200);
  await page.getByRole('button',{name:'最小化',exact:true}).click();await wait(300);assert.equal(await invoke('plugin:window|is_minimized'),true);capture('window-restored');
  report.checks.push('day/week/month interaction','light/dark themes','native maximize, titlebar double-click, minimize and restore');
  await page.getByRole('button',{name:'界面优化讨论',exact:true}).click();await page.waitForSelector('.message');report.session=capture('sessions-compact');
  await page.locator('.reader').evaluate(el=>el.scrollTop=150);await wait(600);
  await page.getByRole('button',{name:'供应商',exact:true}).click();await page.getByRole('button',{name:'会话',exact:true}).click();
  assert.ok(await page.locator('.reader').evaluate(el=>el.scrollTop)>0);
  report.minimum=capture('sessions-minimum',940,620);
  assert.ok(await page.locator('.detail-footer').evaluate(el=>el.getBoundingClientRect().bottom<=innerHeight+1));
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true);
  await page.getByRole('button',{name:'复制线程 ID',exact:true}).click();await page.waitForSelector('.notice');await page.waitForSelector('.notice',{state:'detached',timeout:12000});
  report.checks.push('compact conversation and retained reading position','minimum window footer and overflow','notice expires after ten seconds');
  assert.deepEqual(hashes(),before);assert.deepEqual(report.errors,[]);report.checks.push('synthetic histories unchanged','no JavaScript errors');
  report.status='passed';save();console.log(JSON.stringify({status:report.status,version:report.version,checks:report.checks,output:out}));
 }catch(e){report.status='failed';report.error=e.message;save();throw e}
 finally{if(browser){try{const page=browser.contexts()[0].pages()[0];await page.evaluate(()=>window.__TAURI_INTERNALS__.invoke('plugin:window|close'))}catch{}await browser.close()}if(proc.exitCode===null)proc.kill()}
})().catch(e=>{console.error(e.message);process.exitCode=1});
