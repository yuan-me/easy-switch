export type Period='day'|'week'|'month';
export function localDate(d:Date){return [d.getFullYear(),String(d.getMonth()+1).padStart(2,'0'),String(d.getDate()).padStart(2,'0')].join('-')}
export function periodRange(date:string,period:Period){
 const start=new Date(date+'T00:00:00');
 if(period==='week')start.setDate(start.getDate()-(start.getDay()+6)%7);
 if(period==='month')start.setDate(1);
 const end=new Date(start);
 if(period==='month')end.setMonth(end.getMonth()+1);else end.setDate(end.getDate()+(period==='week'?7:1));
 const bucketStarts:number[]=[];
 for(let d=new Date(start);d<end;){bucketStarts.push(d.getTime()/1000);if(period==='day')d=new Date(d.getTime()+3600000);else d.setDate(d.getDate()+1)}
 return {start,end,bucketStarts};
}
export function dashboardRanges(now=new Date()){
 const today=periodRange(localDate(now),'day');
 const start=new Date(today.start);start.setDate(start.getDate()-6);
 const bucketStarts:number[]=[];
 for(const d=new Date(start);d<today.end;d.setDate(d.getDate()+1))bucketStarts.push(d.getTime()/1000);
 return {today,week:{bucketStarts,end:today.end}};
}
