/** Bound network/setup waits without leaving the UI locked indefinitely. */
export async function withTimeout<T>(operation:Promise<T>, milliseconds:number, message:string):Promise<T> {
 let timer:ReturnType<typeof setTimeout>|undefined;
 try {
  return await Promise.race([operation,new Promise<never>((_,reject)=>{
   timer=setTimeout(()=>reject(new Error(message)),milliseconds);
  })]);
 } finally { if(timer!==undefined)clearTimeout(timer); }
}
export async function fetchSource(url:string):Promise<string> {
 const controller=new AbortController();
 const timer=setTimeout(()=>controller.abort(),15000);
 try {
  const response=await fetch(url,{signal:controller.signal});
  if(!response.ok)throw new Error(`Source HTTP ${response.status}`);
  return await response.text();
 } catch(error) {
  if(controller.signal.aborted)throw new Error('Source loading timed out. Please retry.');
  throw error;
 } finally {clearTimeout(timer);}
}
