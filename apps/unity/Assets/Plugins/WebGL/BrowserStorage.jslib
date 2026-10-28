mergeInto(LibraryManager.library, {
  WorldSimFlushSave: function(path, receiver) {
    var name = UTF8ToString(receiver);
    try {
      FS.syncfs(false, function(error) {
        if(globalThis.worldsimFatal) return;
        SendMessage(name, 'BrowserSaveComplete', error ? String(error) : '');
      });
    } catch (error) { SendMessage(name, 'BrowserSaveComplete', String(error)); }
  },
  WorldSimDownloadSave: function(path) {
    try {
      var bytes = FS.readFile(UTF8ToString(path));
      var url = URL.createObjectURL(new Blob([bytes], {type:'application/octet-stream'}));
      var link = document.createElement('a');
      link.href = url; link.download = 'world-simulator.wssave';
      document.body.appendChild(link); link.click(); link.remove();
      setTimeout(function() { URL.revokeObjectURL(url); }, 1000);
    } catch (error) { alert('Could not download save: ' + error); }
  },
  WorldSimImportSave: function(path, receiver) {
    var dest = UTF8ToString(path), name = UTF8ToString(receiver);
    if (Module.worldsimImportOpen) return;
    Module.worldsimImportOpen = true;
    var input = document.createElement('input');
    input.type = 'file'; input.accept = '.wssave'; input.style.display = 'none';
    var cleanup = function() { input.remove(); Module.worldsimImportOpen = false; };
    input.addEventListener('cancel', cleanup);
    input.addEventListener('change', function() {
      var file = input.files[0];
      if (!file) { cleanup(); return; }
      if (file.size > 16 * 1024 * 1024) {
        SendMessage(name, 'BrowserImportError', 'File exceeds the 16 MiB desktop preview limit.');
        cleanup(); return;
      }
      file.arrayBuffer().then(function(buffer) {
        if(globalThis.worldsimFatal) return;
        FS.writeFile(dest, new Uint8Array(buffer));
        SendMessage(name, 'BrowserImportReady', dest);
      }).catch(function(error) { if(globalThis.worldsimFatal) return; SendMessage(name, 'BrowserImportError', String(error)); }).finally(cleanup);
    });
    document.body.appendChild(input); input.click();
  }
});
