const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {boundary} = require('../../apps/unity/Assets/WebGLTemplates/Desktop/player.js');

test('a real Wasm trap quarantines queued game work and future UI calls', () => {
  const queued = [], errors = [], listeners = {};
  const host = {addEventListener:(name, fn)=>listeners[name]=fn};
  for(const name of ['requestAnimationFrame','setTimeout','setInterval']) host[name]=(fn)=>queued.push(fn);
  const state = boundary(host, error=>errors.push(error));
  let calls = 0;
  const wasm = new WebAssembly.Instance(new WebAssembly.Module(Uint8Array.from([
    0,97,115,109,1,0,0,0,1,4,1,96,0,0,3,2,1,0,7,8,1,4,116,114,97,112,0,0,10,5,1,3,0,0,11
  ])));
  host.requestAnimationFrame(()=>wasm.exports.trap());
  host.requestAnimationFrame(()=>calls++);
  host.setTimeout(()=>calls++, 0);
  host.setInterval(()=>calls++, 1);
  queued.forEach(fn=>fn());
  state.guard(()=>calls++)();
  listeners.unhandledrejection({reason:new Error('secondary failure')});
  assert.equal(calls, 0);
  assert.equal(errors.length, 1);
  assert.ok(errors[0] instanceof WebAssembly.RuntimeError);
  assert.equal(state.isFailed(), true);
});

test('browser save completion waits for syncfs and reports storage failures', () => {
  let plugin, complete;
  const messages = [];
  const context = {
    LibraryManager:{library:{}}, mergeInto:(dest, value)=>plugin=value,
    UTF8ToString:x=>x, FS:{syncfs:(populate, cb)=>{assert.equal(populate,false);complete=cb;}},
    SendMessage:(...args)=>messages.push(args)
  };
  vm.runInNewContext(fs.readFileSync('apps/unity/Assets/Plugins/WebGL/BrowserStorage.jslib','utf8'), context);
  plugin.WorldSimFlushSave('/idbfs/world.wssave','Real Block Demo');
  assert.deepEqual(messages, []);
  complete(new Error('quota exceeded'));
  assert.equal(messages[0][1], 'BrowserSaveComplete');
  assert.match(messages[0][2], /quota exceeded/);
  messages.length=0;
  plugin.WorldSimFlushSave('/idbfs/world.wssave','Real Block Demo');
  complete(null);
  assert.deepEqual(messages, [['Real Block Demo','BrowserSaveComplete','']]);
});

test('document/window input after a trap is blocked, including reload keystrokes', () => {
  const listeners = {}, reload = {}, other = {};
  let failed = false, reloads = 0, ffiCalls = 0;
  const host = {addEventListener:(name, fn, options)=>{assert.equal(options.capture,true);listeners[name]=fn;}};
  require('../../apps/unity/Assets/WebGLTemplates/Desktop/player.js').quarantineInput(host, ()=>failed, reload, ()=>reloads++);
  function dispatch(type, target, key) {
    let stopped = false;
    listeners[type]({target, key, preventDefault(){}, stopImmediatePropagation(){stopped=true;}});
    if(!stopped) ffiCalls++; // A later Unity document/window listener.
  }
  dispatch('keydown',other,'a');assert.equal(ffiCalls,1);
  failed=true;
  for(const name of Object.keys(listeners)) dispatch(name,other,'a');
  dispatch('keydown',reload,'Enter');
  dispatch('click',reload);
  assert.equal(ffiCalls,1);
  assert.equal(reloads,2);
});

test('late file chooser and storage error callbacks never reenter failed Wasm', () => {
  let plugin, changed, complete, removed=0;
  const messages=[];
  const input={style:{},files:[{size:17*1024*1024}],addEventListener:(name,fn)=>{if(name==='change')changed=fn;},click(){},remove(){removed++;}};
  const context={LibraryManager:{library:{}},mergeInto:(dest,value)=>plugin=value,UTF8ToString:x=>x,
    Module:{},document:{createElement:()=>input,body:{appendChild(){}}},
    SendMessage:(...args)=>messages.push(args),FS:{syncfs:(populate,cb)=>complete=cb}};
  vm.runInNewContext(fs.readFileSync('apps/unity/Assets/Plugins/WebGL/BrowserStorage.jslib','utf8'),context);
  plugin.WorldSimImportSave('/idbfs/import','Real Block Demo');
  plugin.WorldSimFlushSave('/idbfs/save','Real Block Demo');
  context.worldsimFatal=true;
  changed();complete(new Error('late quota error'));
  assert.equal(removed,1);assert.deepEqual(messages,[]);
  context.worldsimFatal=false;
  context.FS.syncfs=()=>{context.worldsimFatal=true;throw new Error('abort during sync');};
  plugin.WorldSimFlushSave('/idbfs/save','Real Block Demo');
  assert.deepEqual(messages,[]);
});
