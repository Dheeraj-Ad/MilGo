import { spawn } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { mkdir, rm } from 'node:fs/promises';
import path from 'node:path';
import assert from 'node:assert/strict';
import { chromium } from '@playwright/test';
import { createServer } from 'node:http';
import { DatabaseSync } from 'node:sqlite';

const root=process.cwd();
await mkdir('.local',{recursive:true});
const database=path.join(root,'.local',`integration-${Date.now()}.db`);
const executable=path.join(root,'server','target','debug',process.platform==='win32'?'taskocalypse-server.exe':'taskocalypse-server');
const origin='http://127.0.0.1:8081';
const outbox=[];
const mailer=createServer(async(req,res)=>{
 if(req.method!=='POST'||req.url!=='/emails'||req.headers.authorization!=='Bearer integration-only-key'){res.writeHead(403).end();return;}
 let raw='';for await(const chunk of req)raw+=chunk;
 const payload=JSON.parse(raw);outbox.push(payload);
 res.writeHead(200,{'Content-Type':'application/json'}).end(JSON.stringify({id:'local-test-delivery'}));
});
await new Promise(resolve=>mailer.listen(0,'127.0.0.1',resolve));
const mailPort=mailer.address().port;
const child=spawn(executable,[],{cwd:root,windowsHide:true,env:{...process.env,TOKEN_ENCRYPTION_KEY:randomBytes(32).toString('base64'),DATABASE_PATH:database,APP_URL:origin,BIND_ADDRESS:'127.0.0.1:8081',STATIC_DIR:'dist',GOOGLE_CLIENT_ID:'',GOOGLE_CLIENT_SECRET:'',OTP_API_KEY:'integration-only-key',OTP_FROM:'Town <town@example.com>',OTP_API_URL:`http://127.0.0.1:${mailPort}/emails`},stdio:'ignore'});
let browser;
try{
 let healthy=false;for(let i=0;i<40;i++){try{const r=await fetch(origin+'/api/health');if(r.ok){healthy=true;break;}}catch{}await new Promise(r=>setTimeout(r,250));}assert.ok(healthy,'Rust server starts and responds');
 browser=await chromium.launch({headless:true,args:['--use-angle=swiftshader','--enable-unsafe-swiftshader']});
 const page=await browser.newPage({viewport:{width:1280,height:900}});
 await page.addInitScript(()=>{const getContext=HTMLCanvasElement.prototype.getContext;HTMLCanvasElement.prototype.getContext=function(type,...args){if(type.startsWith('webgl'))return null;return getContext.call(this,type,...args);};});
 await page.goto(origin);
 await page.getByLabel('Your name').fill('Test Neighbour');
 await page.getByLabel('Email address').fill('neighbour@example.com');
 await page.getByLabel('Password',{exact:true}).fill('a unique test passphrase');
 await page.getByRole('checkbox').check();
 await page.getByRole('button',{name:'Start my adventure'}).click();
 await page.getByLabel('Email verification code').waitFor();
 assert.equal((await page.request.get(origin+'/api/auth/me')).status(),401);
 const code=outbox.at(-1).text.match(/code is: (\d{6})/)[1];
 await page.getByLabel('Email verification code').fill(code);
 await page.getByRole('button',{name:'Verify & activate'}).click();
 await page.getByRole('dialog').waitFor({state:'hidden'});
 await page.getByRole('button',{name:'Settings',exact:true}).click();
 await page.getByRole('button',{name:'Connect with Google'}).click();
 await page.getByRole('alert').filter({hasText:'Google sign-in needs OAuth credentials'}).waitFor();
 let me=await page.request.get(origin+'/api/auth/me');assert.equal(me.status(),200);assert.equal((await me.json()).name,'Test Neighbour');
 const denied=await page.request.post(origin+'/api/auth/logout',{headers:{Origin:'https://outside.example','X-Taskocalypse':'town'}});assert.equal(denied.status(),403);
 await page.getByRole('button',{name:'Sign out',exact:true}).click();
 me=await page.request.get(origin+'/api/auth/me');assert.equal(me.status(),401);
 await page.getByRole('button',{name:'Welcome back',exact:true}).click();
 const db=new DatabaseSync(database);db.exec('UPDATE otp_limits SET last_send=0');db.close();
 await page.getByLabel('Email address').fill('neighbour@example.com');
 await page.getByLabel('Password',{exact:true}).fill('a unique test passphrase');
 const loginResponse=page.waitForResponse(r=>r.url().endsWith('/api/auth/login'));
 await page.getByRole('button',{name:'Step back into town'}).click();
 const login=await loginResponse;assert.equal(login.status(),200,await login.text());
 await page.getByLabel('Email verification code').fill(outbox.at(-1).text.match(/code is: (\d{6})/)[1]);
 await page.getByRole('button',{name:'Verify & activate'}).click();
 await page.getByRole('dialog').waitFor({state:'hidden'});
 me=await page.request.get(origin+'/api/auth/me');assert.equal(me.status(),200);
 const deletion=await page.request.delete(origin+'/api/auth/account',{headers:{Origin:origin,'X-Taskocalypse':'town'}});assert.equal(deletion.status(),200);
 assert.equal((await page.request.get(origin+'/api/auth/me')).status(),401);
 assert.equal(outbox.length,2);
 console.log('PASS: live Rust health, registration + emailed OTP, login + new OTP, Google setup error, session cookies, cross-origin rejection, logout, account deletion. No real emails sent.');
}finally{
 await browser?.close();child.kill();await new Promise(r=>child.once('exit',r));await new Promise(r=>mailer.close(r));
 for(const suffix of ['','-shm','-wal'])await rm(database+suffix,{force:true});
}
