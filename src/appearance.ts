import type {Settings} from './types';

export const defaultAccent = '#007acc';
export const accentColors = [
 ['湖蓝',defaultAccent],['蓝色','#087cf0'],['紫色','#a53ee7'],['玫红','#ec255d'],
 ['红色','#e84335'],['橙色','#ee8900'],['黄色','#e6b800'],['绿色','#26a954'],['灰色','#80818a'],
] as const;

export function applyAppearance(settings:Pick<Settings,'theme'|'accentColor'|'colorStyle'>){
 const root=document.documentElement;
 const dark=settings.theme==='dark'||(settings.theme==='system'&&matchMedia('(prefers-color-scheme: dark)').matches);
 root.dataset.theme=dark?'dark':'light';
 root.dataset.colorStyle=settings.colorStyle==='soft'?'soft':'vivid';
 const hex=/^#[\da-f]{6}$/i.test(settings.accentColor??'')?settings.accentColor!:defaultAccent;
 const rgb=[1,3,5].map(i=>parseInt(hex.slice(i,i+2),16));
 const [r,g,b]=rgb.map(n=>n/255),max=Math.max(r,g,b),min=Math.min(r,g,b),delta=max-min;
 const hue=delta===0?0:60*(max===r?(g-b)/delta+(g<b?6:0):max===g?(b-r)/delta+2:(r-g)/delta+4);
 const saturation=delta===0?0:delta/(1-Math.abs(max+min-1));
 root.style.setProperty('--decoration-filter',`hue-rotate(${hue-276.6}deg) saturate(${saturation/.78})`);
 const luminance=(v:number[])=>v.map(n=>{const c=n/255;return c<=.04045?c/12.92:((c+.055)/1.055)**2.4}).reduce((sum,n,i)=>sum+n*[.2126,.7152,.0722][i],0);
 const mix=(factor:number,target:number)=>rgb.map(n=>Math.round(n*(1-factor)+target*factor));
 // Keep labels readable for yellow, custom colors, and both window themes.
 let color=dark?mix(.35,255):rgb;
 for(let amount=.05;!dark&&luminance(color)>.183&&amount<=1;amount+=.05)color=mix(amount,0);
 for(let amount=.4;dark&&luminance(color)<.25&&amount<=1;amount+=.05)color=mix(amount,255);
 root.style.setProperty('--seed',hex);
 root.style.setProperty('--accent',`rgb(${color.join(' ')})`);
 root.style.setProperty('--on-accent',dark?'#201526':'#ffffff');
}
