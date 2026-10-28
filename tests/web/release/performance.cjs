// Measure the actual Unity player; do not confuse a browser engine with a device guarantee.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {execFileSync}=require('node:child_process');
const pw=require(process.env.WORLD_SIM_PLAYWRIGHT||'playwright');
const engine=process.argv[2]||'chromium',seconds=Number(process.argv[3]||300),out=path.resolve(process.argv[4]||'/tmp/worldsim-performance-'+engine);fs.mkdirSync(out,{recursive:true});
const results={engine,headless:process.env.WORLD_SIM_HEADFUL!=='1',secondsPerPhase:seconds,samples:[],failures:[]};let browser;
(async()=>{
 browser=await pw[engine].launch({headless:process.env.WORLD_SIM_HEADFUL!=='1',args:engine==='chromium'&&process.env.WORLD_SIM_METAL==='1'?['--use-angle=metal']:[]});results.version=browser.version();const context=await browser.newContext({viewport:{width:1280,height:900}});
 await context.addInitScript(()=>Object.defineProperty(window,'WorldSimPage',{configurable:true,set(api){const original=api.start;api.start=create=>original((...a)=>create(...a).then(player=>{window.__releasePlayer=player;return player}));Object.defineProperty(window,'WorldSimPage',{value:api,writable:true,configurable:true});}}));
 const page=await context.newPage();page.on('pageerror',e=>results.failures.push(String(e)));await page.goto(process.env.WORLD_SIM_RELEASE_URL||'http://127.0.0.1:8766/world-simulator/');await page.getByText('Ready',{exact:true}).waitFor({timeout:120000});
 results.environment=await page.evaluate(()=>{const gl=document.querySelector('canvas').getContext('webgl2'),debug=gl?.getExtension('WEBGL_debug_renderer_info');return {userAgent:navigator.userAgent,visibility:document.visibilityState,renderer:debug?gl.getParameter(debug.UNMASKED_RENDERER_WEBGL):null,viewport:[innerWidth,innerHeight],devicePixelRatio}});
 if(process.env.WORLD_SIM_REQUIRE_GPU==='1')assert(results.environment.renderer&&!/SwiftShader|llvmpipe|software/i.test(results.environment.renderer),'GPU renderer required');
 async function snapshot(name){const got=page.waitForEvent('download',{timeout:30000});await page.getByRole('button',{name:'Download save',exact:true}).click();const file=path.join(out,name+'.wssave');await(await got).saveAs(file);return JSON.parse(execFileSync('python3',[path.join(__dirname,'inspect_save.py'),process.env.WORLD_SIM_NATIVE_CLI,file],{encoding:'utf8'}));}
 await page.waitForTimeout(10000);
 for(const [phase,x] of [['x1',175],['x100',378]]){
  const r=await page.locator('canvas').boundingBox();await page.mouse.click(r.x+x*r.width/1100,r.y+147*r.width/1100);
  await page.waitForTimeout(1000);const nativeStart=await snapshot(phase+'-start');assert.equal(nativeStart.scale,phase==='x1'?1:100,'HUD click must select the intended speed');
  await page.evaluate(()=>{window.__releaseFrameDeltas=[];let last=performance.now();function sample(t){window.__releaseFrameDeltas.push(t-last);last=t;requestAnimationFrame(sample)}if(!window.__releaseSampler){window.__releaseSampler=true;requestAnimationFrame(sample)}});
  const start=Date.now();while(Date.now()-start<seconds*1000){await page.waitForTimeout(5000);const sample=await page.evaluate(()=>({fatal:!!window.worldsimFatal,visibility:document.visibilityState,metrics:window.worldsimFatal?null:window.__releasePlayer.GetMetricsInfo()}));assert(!sample.fatal,'Player failed during performance run');assert.equal(sample.visibility,'visible');results.samples.push({phase,elapsedMs:Date.now()-start,...sample});fs.writeFileSync(path.join(out,'results.json'),JSON.stringify(results,null,2));}
  const deltas=await page.evaluate(()=>window.__releaseFrameDeltas.splice(0));deltas.sort((a,b)=>a-b);const fps=results.samples.filter(s=>s.phase===phase).map(s=>s.metrics.fps);const nativeEnd=await snapshot(phase+'-end');assert(nativeEnd.tick>nativeStart.tick,'Simulation must advance during benchmark');assert.equal(nativeEnd.scale,nativeStart.scale);results[phase]={nativeStart,nativeEnd,elapsedMs:Date.now()-start,samples:fps.length,meanFps:fps.reduce((a,b)=>a+b,0)/fps.length,minFps:Math.min(...fps),rafP95Ms:deltas[Math.floor(deltas.length*.95)],rafP99Ms:deltas[Math.floor(deltas.length*.99)]};await page.screenshot({path:path.join(out,phase+'.png')});
 }
 results.completed=true;
 console.log(JSON.stringify({engine,environment:results.environment,x1:results.x1,x100:results.x100,first:results.samples[0],last:results.samples.at(-1),failures:results.failures}));
})().catch(e=>{results.failure=String(e.stack||e);console.error(e);process.exitCode=1}).finally(async()=>{fs.writeFileSync(path.join(out,'results.json'),JSON.stringify(results,null,2));await browser?.close()});
