/** Apply the same scalar preset to the downloadable/editable canonical source. */
export function withOverride(source:string,name:string,value:number):string {
 if(!['preset','phase_degrees'].includes(name)||!Number.isFinite(value))throw new Error('Unsupported scene preset');
 const pattern=new RegExp(`(^let ${name} = )[^\\r\\n]+`,'m');
 if(!pattern.test(source))throw new Error(`Source has no ${name} default`);
 return source.replace(pattern,`$1${value}`);
}
export function presetFor(id:string,query:URLSearchParams):[string,number]|null {
 const name=id==='one-rule-many-forms'?'preset':id==='when-waves-meet'?'phase_degrees':'';
 if(!name||!query.has(name))return null;
 const value=Number(query.get(name));
 const choices=name==='preset'?[0,1]:[0,90,180];
 return choices.includes(value)?[name,value]:null;
}
