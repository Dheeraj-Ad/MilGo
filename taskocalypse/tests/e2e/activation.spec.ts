import {test,expect} from '@playwright/test';

test.beforeEach(async({page})=>{
 await page.addInitScript(()=>{const original=HTMLCanvasElement.prototype.getContext;HTMLCanvasElement.prototype.getContext=function(type:string,...args:any[]){if(type.startsWith('webgl'))return null;return (original as any).call(this,type,...args);} as any;});
});

test('OTP is entered before account activation and terms are passed to verify',async({page})=>{
 await page.route('**/api/auth/challenge',r=>r.fulfill({status:401,json:{error:'Sign in'}}));
 await page.route('**/api/auth/me',r=>r.fulfill({status:401,json:{error:'Sign in'}}));
 await page.route('**/api/auth/register',r=>r.fulfill({json:{verificationRequired:true,email:'n***@example.com',expiresAt:Math.floor(Date.now()/1000)+600,resendAfter:Math.floor(Date.now()/1000)+60}}));
 let verification:unknown;
 await page.route('**/api/auth/verify',r=>{verification=r.request().postDataJSON();return r.fulfill({json:{id:'resident',name:'Neighbour',email:'neighbour@example.com',terms:true,gmail:false,xp:0,completed:0,expiresAt:Math.floor(Date.now()/1000)+10800}});});
 await page.goto('/');await page.getByLabel('Your name').fill('Neighbour');await page.getByLabel('Email address').fill('neighbour@example.com');await page.getByLabel('Password',{exact:true}).fill('a unique test password');await page.getByRole('checkbox').check();
 await page.getByRole('button',{name:'Start my adventure'}).click();await expect(page.getByLabel('Email verification code')).toBeVisible();expect(verification).toBeUndefined();
 await page.getByLabel('Email verification code').fill('123456');await page.getByRole('button',{name:'Verify & activate'}).click();await expect(page.getByRole('dialog')).toHaveCount(0);expect(verification).toEqual({code:'123456',terms:true});
 await page.getByRole('button',{name:'Settings',exact:true}).click();await expect(page.getByText(/Access ends in 180 minutes/)).toBeVisible();
});

test('browser expiry clears real inbox content and returns to sign-in',async({page})=>{
 await page.clock.install();
 await page.route('**/api/auth/challenge',r=>r.fulfill({status:401,json:{error:'No pending code'}}));
 const expiry=Math.floor(Date.now()/1000)+30;
 await page.route('**/api/auth/me',r=>r.fulfill({json:{id:'resident',name:'Neighbour',email:'neighbour@example.com',terms:true,gmail:true,xp:10,completed:1,expiresAt:expiry}}));
 await page.route('**/api/mail',r=>r.fulfill({json:{messages:[{id:'private',from:'Private Sender',subject:'Sensitive real letter',body:'Private message',unread:true,community:false}],nextPageToken:null}}));
 await page.goto('/');await expect(page.getByText('Gmail connected',{exact:true})).toBeVisible();await page.getByRole('button',{name:'Say hello to Milo'}).click();await expect(page.getByText('Sensitive real letter')).toBeVisible();
 await page.clock.fastForward(31000);await expect(page.getByRole('button',{name:'Start my adventure'})).toBeVisible();await expect(page.getByText('Sensitive real letter')).toHaveCount(0);await expect(page.getByRole('alert')).toContainText('activation has ended');
});
