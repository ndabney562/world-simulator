/* Fail-stop page boundary: after a fatal error no scheduled game callback runs. */
(function(root) {
  'use strict';
  function boundary(host, onFailure) {
    let failed = false;
    const fail = function(error) {
      if (failed) return;
      failed = true;
      onFailure(error);
    };
    const guard = function(callback) {
      return function() {
        if (failed) return;
        try { return callback.apply(this, arguments); }
        catch (error) { fail(error); }
      };
    };
    ['requestAnimationFrame', 'setTimeout', 'setInterval'].forEach(function(name) {
      const original = host[name].bind(host);
      host[name] = function(callback) {
        const args = Array.from(arguments);
        // Unity uses function callbacks. Reject string timers rather than eval.
        args[0] = typeof callback === 'function' ? guard(callback) : function() {};
        return original.apply(host, args);
      };
    });
    host.addEventListener('error', function(event) { fail(event.error || event.message); });
    host.addEventListener('unhandledrejection', function(event) { fail(event.reason); });
    return {fail: fail, guard: guard, isFailed: function() { return failed; }};
  }
  function start(create) {
    const canvas = document.getElementById('unity-canvas');
    const status = document.getElementById('status');
    const progress = document.getElementById('progress');
    const download = document.getElementById('download'), upload = document.getElementById('upload');
    let instance;
    const state = boundary(window, function(error) {
      root.worldsimFatal = true;
      console.error('World Simulator stopped:', error);
      document.getElementById('fault').hidden = false;
      canvas.hidden = true; canvas.style.pointerEvents = 'none';
      download.disabled = upload.disabled = true;
      instance = null; // Never call Quit/dispose/FFI on a failed Wasm instance.
    });
    document.getElementById('reload').onclick = function() { location.reload(); };
    ['keydown','keyup','pointerdown','pointerup','mousedown','mouseup','wheel','touchstart'].forEach(function(name) {
      canvas.addEventListener(name, function(event) {
        if(state.isFailed()) { event.preventDefault(); event.stopImmediatePropagation(); }
      }, {capture:true, passive:false});
    });
    function send(method) {
      if(instance && !state.isFailed()) state.guard(function() { instance.SendMessage('Real Block Demo', method); })();
    }
    download.onclick = function() { send('ExportBrowserSave'); };
    upload.onclick = function() { send('ImportBrowserSave'); };
    try {
      create(canvas, function(value) { progress.value = value; }, state.fail).then(function(player) {
        if(state.isFailed()) return;
        instance = player; status.textContent = 'Ready'; progress.hidden = true;
        download.disabled = upload.disabled = false; canvas.focus();
      }).catch(state.fail);
    } catch(error) { state.fail(error); }
  }
  root.WorldSimPage = {boundary: boundary, start: start};
  if(typeof module !== 'undefined') module.exports = root.WorldSimPage;
})(globalThis);
