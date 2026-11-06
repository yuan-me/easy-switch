const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');

module.exports = async function () {
  const window = {};
  vm.runInNewContext(fs.readFileSync(require.resolve('./native-fast-unlock.js'), 'utf8'), {window, URL});
  const api = window.__codexPlusNativeFastUnlock;
  api.runtime.update({enhancementsEnabled:true,codexAppNativeFastUnlock:true,nativeFastLocalApiConfigured:true});
  const ui = 'export const __webpack_modules__={menu(e,t,n){var a=n("auth");n.d(t,{},{U:function(e){let p=e?.hostId??fallback,m=(0,a.auth)(p),h=m?.authMethod==="chatgpt"||m?.authMethod==="personalAccessToken"||m?.authMethod==="amazonBedrock",g=m?.authMethod??null;let q={authMethod:g,hostId:p};return {serviceTierAccess:h}}})}};';
  const request = 'export const request=async function(e,h){let a=await (0,x.auth)(e,h);if("chatgpt"!==a&&"personalAccessToken"!==a&&"amazonBedrock"!==a)return null;let r=await (0,x.requirements)(e,h,{priority:"critical"});return e.query.setData(x.key,{authMethod:a,hostId:h},r)};';
  const queue = 'export function defaults(e,r){return{async readServiceTier(){return read(async()=>{if("local"===e.getHostId()&&await r.isUsingCopilotInference())return!1;let a=await e.getAccount({priority:"critical"});return a.account?.type==="chatgpt"||a.account?.type==="amazonBedrock"})},async writeServiceTier(){}}};export const queued={prepare:function(e,{settings:s,storage:g,filesystem:f,runtime:r},a,b,c){return async(x,m)=>{let o=m.submissionOptions,h=o?.executionHostId??e.getHostId();return(async()=>{if(null==o)return a();if(null==o.serviceTier)return null;let{account:t}=await e.getAccount({priority:"critical"});return t?.type==="chatgpt"||t?.type==="amazonBedrock"?o.serviceTier:null})()}}};';
  const urls = [1,2,3,4].map(n=>`app://-/assets/async/${n}.abc.js`);
  const run = async (sources, moduleUrls=urls) => {
    const fetched=[];
    const result=await api.prepareDocumentModules({
      location:new URL('app://-/index.html'),document:{scripts:[{src:'app://-/assets/123.abc.js'}]},moduleUrls,
      fetchText:async url=>{fetched.push(url);return sources[urls.indexOf(url)]??null},
    });
    return {result,fetched};
  };
  const {result,fetched}=await run([ui,request,queue,ui], [...urls,urls[0],'https://example.com/assets/async/5.abc.js']);
  assert.equal((await run([],[])).result.diagnostics.reason,'incomplete-gates','首次启动还没有异步模块时允许等待');
  assert.equal(result.ok,true,JSON.stringify(result.diagnostics));
  assert.equal(result.modules.length,4);
  assert.equal(result.diagnostics.matches['ui-access'],2,'同一模块的副本都改写');
  assert.deepEqual(fetched,urls,'不重复读取，不访问远程模块');
  for(const item of result.modules)assert.ok(item.patchedSource.includes('__codexPlusNativeFastCompiled="v1"'));
  for(const sources of [[ui,null,queue,ui],[ui,request,queue,ui.replace('menu(e,t,n)','other(e,t,n)')],[ui,request,queue.replace('isUsingCopilotInference','unknownCopilot'),ui]]){
    const {result}=await run(sources);
    assert.equal(result.ok,false);
    assert.equal(result.modules.length,0,'缺少判断或模块不明确时不能部分应用');
  }
  // 执行改写后的两条队列路径，验证 API 身份、远端和 Copilot 的行为。
  const patched=result.modules.find(m=>m.url===urls[2]).patchedSource.replaceAll('export ','');
  const context=vm.createContext({window,read:fn=>fn()});
  vm.runInContext(patched,context);
  for(const [host,copilot,expected] of [['local',false,'priority'],['remote',false,null],['local',true,null]]){
    const manager={getHostId:()=>host,getAccount:async()=>({account:{type:'apiKey'}})};
    const runtime={isUsingCopilotInference:async()=>copilot};
    assert.equal(await context.defaults(manager,runtime).readServiceTier(),expected!==null);
    context.manager=manager;context.runtime=runtime;
    assert.equal(await vm.runInContext('queued.prepare(manager,{settings:{},storage:{},filesystem:{},runtime},()=>null,()=>{},()=>{})("test",{submissionOptions:{executionHostId:manager.getHostId(),serviceTier:"priority"}})',context),expected);
  }
  api.runtime.update({enhancementsEnabled:true,codexAppNativeFastUnlock:false});
  assert.equal((await run([ui,request,queue,ui])).result.ok,false);
};
