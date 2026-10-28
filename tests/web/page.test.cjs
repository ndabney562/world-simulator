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
