// Runs the real exported player in disposable profiles; no production hooks.
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const crypto = require('node:crypto');
const {execFileSync} = require('node:child_process');
const assert = require('node:assert/strict');
const playwright = require(process.env.WORLD_SIM_PLAYWRIGHT || 'playwright');
const engine = process.argv[2] || 'chromium';
const out = path.resolve(process.argv[3] || path.join(os.tmpdir(), 'worldsim-release-'+engine));
const base = process.env.WORLD_SIM_RELEASE_URL || 'http://127.0.0.1:8766/world-simulator/';
const cli = process.env.WORLD_SIM_NATIVE_CLI;
assert(cli && fs.existsSync(cli), 'WORLD_SIM_NATIVE_CLI must identify the built native validator');
fs.mkdirSync(out,{recursive:true});
const results={engine,base,checks:[],errors:[]};
const active=[];
function check(name,data={}) {results.checks.push({name,...data}); console.log(engine,name,JSON.stringify(data));fs.writeFileSync(path.join(out,'results.json'),JSON.stringify(results,null,2));}
async function instrumentation(context) {
 await context.addInitScript(()=>{
  Object.defineProperty(window,'WorldSimPage',{configurable:true,set(api){
   const original=api.start;
   api.start=create=>original((...args)=>create(...args).then(player=>{window.__releasePlayer=player;return player}));
   Object.defineProperty(window,'WorldSimPage',{value:api,writable:true,configurable:true});
  }});
 });
}
async function ready(page) {await page.goto(base);await page.getByText('Ready',{exact:true}).waitFor({timeout:120000});await page.waitForTimeout(2500);assert.equal(await page.evaluate(()=>!!window.worldsimFatal),false);}
async function hud(page,button) {const box=await page.locator('canvas').boundingBox();const scale=box.width/1100;const coords={pause:[66,147],x1:[175,147],x10:[275,147],x100:[378,147],save:[288,172],load:[385,172],street:[177,172],overview:[77,172]};const [x,y]=coords[button];await page.mouse.click(box.x+x*scale,box.y+y*scale);await page.waitForTimeout(700);}
function stateKey(save){const match=save.native.match(/loaded tick=\d+ state=[^ ]+/);assert(match,'Native loader did not report state');return match[0];}
async function screenshot(page,name) {await page.screenshot({path:path.join(out,name+'.png')});}
async function download(page,name) {const got=page.waitForEvent('download',{timeout:30000});await page.getByRole('button',{name:'Download save',exact:true}).click();const d=await got;const file=path.join(out,name+'.wssave');await d.saveAs(file);let native=null;if(cli)native=execFileSync(cli,['load','--file',file,'--ticks','0'],{encoding:'utf8'});return {file,sha256:crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex'),native};}
(async()=>{
 const type=playwright[engine];assert(type,'Unknown browser engine');
 const profile=fs.mkdtempSync(path.join(os.tmpdir(),'worldsim-'+engine+'-'));
 const options={headless:true,viewport:{width:1280,height:900},acceptDownloads:true};
 let context=await type.launchPersistentContext(profile,options);active.push(context);await instrumentation(context);let page=await context.newPage();
 page.on('pageerror',e=>results.errors.push(String(e)));
 await ready(page);check('cold-load',{version:context.browser()?.version(),userAgent:await page.evaluate(()=>navigator.userAgent),metrics:await page.evaluate(()=>window.__releasePlayer.GetMetricsInfo())});
 await hud(page,'pause');await screenshot(page,'paused');const first=await download(page,'saved');check('download-valid-save',first);
 await context.close();context=await type.launchPersistentContext(profile,options);active.push(context);await instrumentation(context);page=await context.newPage();await ready(page);await hud(page,'load');await screenshot(page,'loaded-after-browser-restart');const second=await download(page,'reloaded');assert.equal(stateKey(second),stateKey(first));check('load-after-browser-restart',second);
 // A rejected import must preserve the current world and keep the page usable.
 const chooser=page.waitForEvent('filechooser');await page.getByRole('button',{name:'Import save',exact:true}).click();await (await chooser).setFiles({name:'invalid.wssave',mimeType:'application/octet-stream',buffer:Buffer.from('invalid save')});await page.waitForTimeout(1500);assert.equal(await page.evaluate(()=>!!window.worldsimFatal),false);await screenshot(page,'invalid-import');const third=await download(page,'after-invalid');assert.equal(stateKey(third),stateKey(first));check('invalid-import-preserves-live-world',third);
 await hud(page,'x1');await page.waitForTimeout(2500);await hud(page,'pause');const changed=await download(page,'changed-before-import');assert.notEqual(stateKey(changed),stateKey(first));
 const valid=page.waitForEvent('filechooser');await page.getByRole('button',{name:'Import save',exact:true}).click();await (await valid).setFiles(first.file);await page.waitForTimeout(2000);assert.equal(await page.evaluate(()=>!!window.worldsimFatal),false);await screenshot(page,'valid-import');const imported=await download(page,'after-valid-import');assert.equal(stateKey(imported),stateKey(first));check('valid-import-restores-different-live-world',{before:stateKey(changed),after:stateKey(imported)});await context.close();context=await type.launchPersistentContext(profile,options);active.push(context);await instrumentation(context);page=await context.newPage();await ready(page);await hud(page,'load');const importedReload=await download(page,'imported-after-browser-restart');assert.equal(stateKey(importedReload),stateKey(first));check('valid-import-survives-browser-restart',importedReload);
 // Delay genuine IndexedDB transaction completion: download must wait for durability.
 await page.evaluate(()=>{const original=IDBDatabase.prototype.transaction;window.__releaseOriginalTransaction=original;IDBDatabase.prototype.transaction=function(...args){const tx=original.apply(this,args);let fn;Object.defineProperty(tx,'oncomplete',{configurable:true,get(){return fn},set(value){fn=value;tx.addEventListener('complete',e=>setTimeout(()=>value?.call(tx,e),3000),{once:true});}});return tx;};});
 const start=Date.now();await download(page,'delayed-durable-save');const elapsed=Date.now()-start;assert(elapsed>=2900,'Save download acknowledged before delayed transaction completion');check('durability-ack-waits-for-indexeddb',{elapsedMs:elapsed});await page.evaluate(()=>{IDBDatabase.prototype.transaction=window.__releaseOriginalTransaction;});
 // Simulate the browser's quota error at the actual IndexedDB write boundary.
 await hud(page,'x1');await page.waitForTimeout(2500);await hud(page,'pause');
 await page.evaluate(()=>{window.__releaseQuotaHits=0;window.__releaseOriginalPut=IDBObjectStore.prototype.put;IDBObjectStore.prototype.put=function(){window.__releaseQuotaHits++;throw new DOMException('Release-check simulated quota exceeded','QuotaExceededError')};});
 const quotaBackup=await download(page,'quota-backup');const quotaHits=await page.evaluate(()=>window.__releaseQuotaHits);assert(quotaHits>0,'Quota injection did not reach an IndexedDB write');assert.equal(await page.evaluate(()=>!!window.worldsimFatal),false);await screenshot(page,'quota-failure');
 check('quota-failure-valid-backup-and-warning-captured',{quotaHits,...quotaBackup,warningScreenshot:'quota-failure.png',visualReviewRequired:true});
 await page.evaluate(()=>{IDBObjectStore.prototype.put=window.__releaseOriginalPut;});const recovered=await download(page,'after-storage-recovery');assert.notEqual(stateKey(recovered),stateKey(first));check('storage-recovery-without-reload',recovered);await context.close();context=await type.launchPersistentContext(profile,options);active.push(context);await instrumentation(context);page=await context.newPage();await ready(page);await hud(page,'load');const recoveredReload=await download(page,'storage-recovery-after-browser-restart');assert.equal(stateKey(recoveredReload),stateKey(recovered));check('recovered-storage-is-durable-after-browser-restart',recoveredReload);
 await context.close();
 // Denied storage is a fresh origin/profile, not a deletion of the user's saves.
 const denied=await type.launch({headless:true});active.push(denied);const dc=await denied.newContext(options);await instrumentation(dc);await dc.addInitScript(()=>{IDBFactory.prototype.open=function(){throw new DOMException('Release-check storage denied','SecurityError')}});const dp=await dc.newPage();await dp.goto(base);await dp.waitForTimeout(15000);if(await dp.locator('#status').textContent()==='Ready'){await download(dp,'denied-storage-backup');await dp.waitForTimeout(1000);}await screenshot(dp,'storage-denied');check('storage-denied-observation',{status:await dp.locator('#status').textContent(),fatal:await dp.evaluate(()=>!!window.worldsimFatal)});await denied.close();
 results.completed=true;fs.writeFileSync(path.join(out,'results.json'),JSON.stringify(results,null,2));
})().catch(async e=>{results.failure=String(e.stack||e);fs.writeFileSync(path.join(out,'results.json'),JSON.stringify(results,null,2));console.error(e);await Promise.all(active.map(c=>c.close().catch(()=>{})));process.exitCode=1;});
