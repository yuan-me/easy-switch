import {setTheme} from './helpers';
import {test,expect,type Page,type Locator} from '@playwright/test';

async function inspectPicker(page:Page,field:Locator,name:string,color:string){
 await expect(field).toHaveCSS('appearance','base-select');
 await field.click();
 await expect.poll(()=>field.evaluate(el=>el.matches(':open'))).toBe(true);
 const style=await field.evaluate(el=>{const s=getComputedStyle(el,'::picker(select)');return {background:s.backgroundColor,radius:s.borderRadius,border:s.borderWidth}});
 expect(style).toEqual({background:color,radius:'10px',border:'1px'});
 const option=field.locator('option:not([hidden])').first();
 const box=await option.boundingBox();expect(box).not.toBeNull();
 const viewport=page.viewportSize()!;expect(box!.x).toBeGreaterThanOrEqual(0);expect(box!.x+box!.width).toBeLessThanOrEqual(viewport.width);expect(box!.y).toBeGreaterThanOrEqual(0);expect(box!.y+box!.height).toBeLessThanOrEqual(viewport.height);
 await page.screenshot({path:`artifacts/screenshots/dropdown-${name}.png`});
 await page.keyboard.press('Escape');
 await expect.poll(()=>field.evaluate(el=>el.matches(':open'))).toBe(false);
 await expect(field).toBeFocused();
}

for(const [theme,color] of [['浅色','rgb(255, 252, 255)'],['深色','rgb(44, 39, 50)']])test(`all selection menus share their surface and fit the minimum window: ${theme}`,async({page})=>{
 const errors:string[]=[];page.on('pageerror',e=>errors.push(e.message));
 await page.setViewportSize({width:940,height:620});await page.goto('/');
 await page.getByRole('button',{name:'供应商',exact:true}).click();await expect(page.locator('.provider-card')).toHaveCount(4);await setTheme(page,theme);
 const card=page.locator('.provider-card').filter({has:page.getByRole('heading',{name:'Sub2API'})});
 await card.getByRole('button',{name:'编辑',exact:true}).click();
 const dialog=page.getByRole('dialog'),mode=dialog.getByLabel('认证模式');
 await inspectPicker(page,mode,theme+'-mode',color);await expect(dialog).toBeVisible();
 await inspectPicker(page,dialog.getByLabel('协议'),theme+'-protocol',color);
 await mode.selectOption('Aggregate');
 await inspectPicker(page,dialog.getByLabel('路由策略'),theme+'-route',color);
 await page.keyboard.press('Escape');await expect(dialog).not.toBeVisible();
 const official=page.locator('.provider-card').filter({has:page.getByRole('heading',{name:'OpenAI 官方',exact:true})});
 await official.getByRole('button',{name:'编辑',exact:true}).click();await expect(dialog.getByLabel('认证模式')).toBeDisabled();await page.keyboard.press('Escape');
 await card.locator('summary').click();
 await expect(card.locator('.card-menu>div')).toHaveCSS('background-color',color);await expect(card.locator('.card-menu>div')).toHaveCSS('border-radius','14px');await card.locator('summary').click();
 await page.getByRole('button',{name:'会话',exact:true}).click();
 await inspectPicker(page,page.getByLabel('项目筛选'),theme+'-project',color);
 await inspectPicker(page,page.getByLabel('会话状态筛选'),theme+'-status',color);
 const status=page.getByLabel('会话状态筛选');await status.press('Space');await page.keyboard.press('End');await page.keyboard.press('Enter');await expect(status).toHaveValue('archived');
 await page.getByRole('button',{name:'Token 统计',exact:true}).click();await expect(page.getByLabel('统计模型').locator('option')).toHaveCount(5);
 await inspectPicker(page,page.getByLabel('统计模型'),theme+'-usage',color);
 await page.getByRole('button',{name:'设置',exact:true}).click();
 await inspectPicker(page,page.getByLabel('配色风格',{exact:true}),theme+'-theme',color);
 expect(errors).toEqual([]);
});

test('editor starts a composited entrance and preserves focus and reduced motion',async({page})=>{
 await page.goto('/');await page.getByRole('button',{name:'供应商',exact:true}).click();await expect(page.getByRole('button',{name:'添加供应商',exact:true})).toBeEnabled();
 await page.getByRole('button',{name:'添加供应商',exact:true}).focus();
 const motion=await page.evaluate(async()=>{
  [...document.querySelectorAll('button')].find(el=>el.textContent?.includes('添加供应商'))!.click();
  for(let i=0;i<3;i++)await new Promise(requestAnimationFrame);
  const dialog=document.querySelector('dialog')!,animation=dialog.getAnimations().find(a=>(a as CSSTransition).transitionProperty==='transform')!;
  const timing=animation?.effect?.getTiming();const initial=new DOMMatrix(getComputedStyle(dialog).transform).m42;
  await animation?.finished;
  return {initial,final:new DOMMatrix(getComputedStyle(dialog).transform).m42,duration:timing?.duration,opacity:getComputedStyle(dialog).opacity};
 });
 expect(motion.initial).toBeGreaterThan(0);expect(motion.initial).toBeLessThanOrEqual(12);expect(motion.final).toBe(0);expect(motion.duration).toBe(260);expect(motion.opacity).toBe('1');
 await expect(page.getByLabel('显示名称',{exact:true})).toBeFocused();
 await page.keyboard.press('Escape');await expect(page.getByRole('button',{name:'添加供应商',exact:true})).toBeFocused();
 await page.emulateMedia({reducedMotion:'reduce'});await page.getByRole('button',{name:'添加供应商',exact:true}).click();
 await expect(page.getByRole('dialog')).toHaveCSS('transition-duration','0s');await expect(page.getByRole('dialog')).toHaveCSS('transform','matrix(1, 0, 0, 1, 0, 0)');
 await page.keyboard.press('Escape');
});


test('long project labels and scrollable menus retain keyboard and high-contrast support',async({page})=>{
 await page.addInitScript(()=>{
  Object.assign(window,{isTauri:true,__TAURI_EVENT_PLUGIN_INTERNALS__:{unregisterListener:()=>{}},__TAURI_INTERNALS__:{
   metadata:{currentWindow:{label:'main'},currentWebview:{label:'main'}},transformCallback:()=>1,
   invoke:async(command:string)=>{
    if(command==='usage_report')return {input:0,output:0,cached:null,buckets:[],models:[],sessions:[],availableModels:[],warnings:[],scannedFiles:0};
    if(command==='interrupted_operations')return [];
    if(command==='bootstrap')return {providers:[],settings:{theme:'light',scrollPositions:{}},version:'test',migrationError:null};
    if(command==='scan_sessions')return {sessions:Array.from({length:40},(_,i)=>({id:String(i),title:'Test '+i,cwd:'D:/very-long-project-name-should-not-hide-the-arrow-'+i,updated:0,archived:false})),warnings:[]};
    return null;
   }
  }});
 });
 await page.setViewportSize({width:940,height:620});await page.goto('/');await page.getByRole('button',{name:'会话',exact:true}).click();
 const project=page.getByLabel('项目筛选');await expect(project.locator('option')).toHaveCount(41);
 await project.selectOption({index:1});
 const label=project.locator('selectedcontent');await expect(label).toHaveCSS('text-overflow','ellipsis');
 expect(await project.evaluate(el=>{const text=el.querySelector('selectedcontent')!.getBoundingClientRect(),field=el.getBoundingClientRect();return field.right-text.right})).toBeGreaterThanOrEqual(24);
 await page.screenshot({path:'artifacts/screenshots/dropdown-long-closed.png'});
 await project.click();await page.keyboard.press('End');await page.screenshot({path:'artifacts/screenshots/dropdown-long-open.png'});await page.keyboard.press('Enter');
 await expect(project).toHaveValue('D:/very-long-project-name-should-not-hide-the-arrow-39');
 await expect(page.locator('.session-row')).toHaveCount(1);
 await page.emulateMedia({forcedColors:'active'});await project.click();
 expect(await project.evaluate(el=>getComputedStyle(el,'::picker(select)').borderStyle)).toBe('solid');
 await page.screenshot({path:'artifacts/screenshots/dropdown-high-contrast.png'});await page.keyboard.press('Escape');await expect(project).toBeFocused();
});
