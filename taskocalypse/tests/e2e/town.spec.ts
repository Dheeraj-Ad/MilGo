import {test,expect} from '@playwright/test';
test('sign-in loads before the optional 3D renderer',async({page})=>{
 await page.goto('/');
 await expect(page.getByRole('dialog')).toBeVisible();
 const loaded=await page.evaluate(()=>performance.getEntriesByType('resource').map(entry=>entry.name));
 expect(loaded.some(url=>/\/three-[^/]+\.js/.test(url))).toBe(false);
});
test('automatic theme follows local time and manual override is available',async({page})=>{
 const nightfall=new Date();nightfall.setHours(20,0,0,0);
 await page.clock.install({time:nightfall});
 await page.addInitScript(()=>localStorage.setItem('tq-demo','true'));
 await page.goto('/');
 await expect(page.locator('.app-shell')).toHaveClass(/theme-night/);
 await page.getByRole('button',{name:'Switch to day mode'}).click();
 await expect(page.locator('.app-shell')).not.toHaveClass(/theme-night/);
 await expect.poll(()=>page.evaluate(()=>localStorage.getItem('tq-theme'))).toBe('"day"');
 await page.getByRole('button',{name:'Settings',exact:true}).click();
 await page.getByLabel('Theme schedule').selectOption('auto');
 await expect(page.locator('.app-shell')).toHaveClass(/theme-night/);
});
test('onboarding, 3D town, practice reply, code quest, and preferences',async({page})=>{
 const errors:string[]=[];page.on('pageerror',e=>errors.push(e.message));
 await page.goto('/');await expect(page.getByRole('dialog')).toBeVisible();
 await page.getByRole('button',{name:/Try the demo/}).click();await expect(page.getByRole('dialog')).toHaveCount(0);
 await expect(page.locator('canvas')).toBeVisible();await expect(page.getByRole('button',{name:'Talk to Milo, Your mail courier'})).toBeVisible();
 await page.keyboard.down('w');await page.waitForTimeout(700);await page.keyboard.up('w');await expect(page.getByText('1 of 6 discovered')).toBeVisible();
 await page.screenshot({path:'test-results/town-desktop.png',fullPage:true});
 await page.getByRole('button',{name:'Say hello to Milo'}).click();await expect(page.getByText('Demo letters ·')).toBeVisible();
 await page.getByRole('button',{name:/Aarav Shah/}).click();await page.getByPlaceholder('Hey! Lovely to hear from you…').fill('Friday sounds lovely. See you there!');await page.getByRole('button',{name:'Send practice reply'}).click();await expect(page.getByRole('status')).toContainText('Practice reply completed');
 await page.getByRole('button',{name:'Close dialog'}).click();await page.getByRole('button',{name:'Code cottage'}).click();
 await page.locator('.code-input-label textarea').fill('const hello = true;  \n');await page.getByRole('button',{name:'Find loose ends'}).click();await expect(page.getByText('1 little fixes found')).toBeVisible();
 const download=page.waitForEvent('download');await page.getByRole('button',{name:'Fix & download clean file'}).click();expect((await download).suggestedFilename()).toBe('snippet.clean.js');
 await page.getByRole('button',{name:'Close dialog'}).click();await page.getByRole('button',{name:'Settings',exact:true}).click();await page.getByLabel('Theme schedule').selectOption('night');await expect(page.locator('.app-shell')).toHaveClass(/theme-night/);await expect(page.locator('.town-canvas')).toHaveClass(/night/);expect(await page.evaluate(()=>localStorage.getItem('tq-theme'))).toBe('"night"');await page.getByLabel('Theme schedule').selectOption('auto');expect(await page.evaluate(()=>localStorage.getItem('tq-theme'))).toBe('"auto"');await page.getByRole('switch',{name:'Reduce motion'}).click();
 expect(errors).toEqual([]);
});
test('expense CSV review and export',async({page})=>{await page.addInitScript(()=>localStorage.setItem('tq-demo','true'));await page.goto('/');await page.getByRole('button',{name:'Coin corner'}).click();await page.locator('input[type=file]').setInputFiles({name:'ledger.csv',mimeType:'text/csv',buffer:Buffer.from('description,amount\nTea,30\nTrain,100')});await page.getByLabel('Category for Tea').selectOption('Food & drink');await page.getByLabel('Category for Train').selectOption('Transport');const event=page.waitForEvent('download');await page.getByRole('button',{name:'Export reviewed ledger'}).click();expect((await event).suggestedFilename()).toBe('reviewed-expenses.csv');await expect(page.getByRole('status')).toContainText('2 expenses reviewed');});
test('mobile layout and announcement board',async({page})=>{await page.setViewportSize({width:390,height:844});await page.addInitScript(()=>localStorage.setItem('tq-demo','true'));await page.goto('/');await expect(page.locator('canvas')).toBeVisible();await page.screenshot({path:'test-results/town-mobile.png',fullPage:true});expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBeTruthy();await page.getByRole('button',{name:/HEAR YE/}).click();await page.getByRole('button',{name:/Campus Creators/}).click();await expect(page.getByText(/Saturday · 4 PM/)).toBeVisible();await page.getByRole('button',{name:'Mark as read'}).click();await expect(page.getByRole('status')).toContainText('Demo message read');});
