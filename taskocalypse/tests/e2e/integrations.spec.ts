import {test,expect} from '@playwright/test';

// These tests exercise data flows. The town's actual WebGL rendering is covered in town.spec.ts.
test.beforeEach(async({page})=>{await page.addInitScript(()=>{const original=HTMLCanvasElement.prototype.getContext;HTMLCanvasElement.prototype.getContext=function(type:string,...args:any[]){if(type.startsWith('webgl')||type==='experimental-webgl')return null;return (original as any).call(this,type,...args);} as any;});});

test('folder plan requires a click before verified copies replace originals',async({page})=>{
 await page.addInitScript(()=>{
  localStorage.setItem('tq-demo','true');
  const original=new Map([['photo.png',new File(['picture'],'photo.png')],['notes.txt',new File(['notes'],'notes.txt')]]);
  const moved:Record<string,string[]>={};
  (window as any).__folder={original,moved};
  const root={name:'My test folder',async *entries(){for(const[name,file]of original)yield[name,{kind:'file',getFile:async()=>file}];},async getDirectoryHandle(category:string){moved[category]??=[];const output=new Map<string,File>();return{async getFileHandle(name:string,options?:{create:boolean}){if(!options?.create&&!output.has(name))throw new DOMException('missing','NotFoundError');return{async createWritable(){return{async write(file:File){output.set(name,file);moved[category].push(name);},async close(){}};},async getFile(){return output.get(name);}};}};},async removeEntry(name:string){original.delete(name);}};
  Object.defineProperty(window,'showDirectoryPicker',{value:async()=>root,configurable:true});
 });
 await page.goto('/');await page.getByRole('button',{name:'File grove'}).click();await page.getByRole('button',{name:'Choose a folder'}).click();
 await expect(page.getByRole('cell',{name:'photo.png',exact:true})).toBeVisible();
 expect(await page.evaluate(()=>(window as any).__folder.original.size)).toBe(2);
 await page.getByRole('button',{name:'Move 2 files into their new homes'}).click();await expect(page.getByText('Your files are now in their category folders.')).toBeVisible();
 expect(await page.evaluate(()=>(window as any).__folder.original.size)).toBe(0);
 expect(await page.evaluate(()=>(window as any).__folder.moved)).toEqual({Images:['photo.png'],Documents:['notes.txt']});
});

test('connected inbox supports pagination and explicit reply payloads',async({page})=>{
 const first={id:'real-1',threadId:'thread-1',from:'Milo <milo@example.com>',subject:'First real-format letter',body:'Hello from the mocked Gmail API.',date:'Today',unread:true,community:false};
 await page.route('**/api/auth/me',r=>r.fulfill({json:{id:'test-user',email:'player@example.com',name:'Player',gmail:true,terms:true,xp:0,completed:0}}));
 await page.route('**/api/mail?*',r=>r.fulfill({json:{messages:[{...first,id:'real-2',subject:'An older letter'}],nextPageToken:null}}));
 await page.route('**/api/mail',r=>r.fulfill({json:{messages:[first],nextPageToken:'next-page'}}));
 let sent:unknown;
 await page.route('**/api/mail/real-2/reply',r=>{sent=r.request().postDataJSON();return r.fulfill({json:{ok:true,xp:40}});});
 await page.goto('/');await expect(page.getByText('Gmail connected',{exact:true})).toBeVisible();
 await page.getByRole('button',{name:'Say hello to Milo'}).click();await page.getByRole('button',{name:'Load more letters'}).click();
 await page.getByRole('button',{name:/An older letter/}).click();await page.getByPlaceholder('Hey! Lovely to hear from you…').fill('Thanks, Milo.');
 expect(sent).toBeUndefined();await page.getByRole('button',{name:'Send reply via Gmail'}).click();await expect(page.getByRole('status')).toContainText('Reply sent through Gmail.');expect(sent).toEqual({body:'Thanks, Milo.'});
});
