#!/usr/bin/env python3
"""Build a separate real-Rust-panic Unity player; restore every patched input.

Never distribute Builds/ReleaseFaultProbe. The normal DesktopWeb output is untouched.
The test-only delta makes x100 call Rust panic! through the normal slice ABI.
"""
from pathlib import Path
import hashlib
import json
import shutil
import subprocess
import signal

ROOT = Path(__file__).resolve().parents[3]
RUST = ROOT / 'core/crates/ws-ffi/src/slice_bridge.rs'
EDITOR = ROOT / 'apps/unity/Assets/WorldSim/Editor/WebBuild.cs'
PLUGIN = ROOT / 'apps/unity/Assets/Plugins/WebGL/libws_ffi.a'
OUT = ROOT / 'apps/unity/Builds/ReleaseFaultProbe'


def main():
    # Run restoration even if the test driver is asked to terminate.
    signal.signal(signal.SIGTERM, lambda *_: (_ for _ in ()).throw(SystemExit(143)))
    subprocess.run(['git', 'diff', '--exit-code', 'HEAD', '--', str(RUST), str(EDITOR)], cwd=ROOT, check=True)
    original = {p: p.read_bytes() for p in (RUST, EDITOR, PLUGIN)}
    rust = original[RUST].decode()
    needle = '100 => ws_sim::TimeScale::X100,'
    assert rust.count(needle) == 1
    replacement = '100 => panic!("WORLD_SIM_RELEASE_PROBE_RUST_PANIC"),'
    editor = original[EDITOR].decode()
    location = 'locationPathName="Builds/DesktopWeb"'
    assert editor.count(location) == 1
    try:
        RUST.write_text(rust.replace(needle, replacement))
        EDITOR.write_text(editor.replace(location, 'locationPathName="Builds/ReleaseFaultProbe"'))
        subprocess.run(['scripts/build-native-webgl.sh'], cwd=ROOT, check=True)
        subprocess.run(['python3', 'scripts/run_unity_web.py'], cwd=ROOT, check=True)
        for name in ('THIRD_PARTY_NOTICES.txt', 'UNITY_PLAYER_NOTICES.pdf', 'RUST_LIBRARY_COPYRIGHT.html'):
            shutil.copy2(ROOT / 'apps/unity/Builds/DesktopWeb' / name, OUT / name)
        index = OUT / 'index.html'
        index.write_text(index.read_text().replace('desktop preview', 'FAULT-INJECTION TEST — DO NOT DISTRIBUTE'))
        (OUT / 'fault-probe.json').write_text(json.dumps({
            'test_only': True,
            'source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
            'source_rust_sha256': hashlib.sha256(original[RUST]).hexdigest(),
            'injection': replacement,
            'trigger': 'x100 HUD button through normal Unity -> Rust slice operation',
        }, indent=2) + '\n')
    finally:
        for p, content in original.items():
            p.write_bytes(content)
        assert all(p.read_bytes() == content for p, content in original.items())
    print('FAULT_PROBE_BUILT; source inputs and production native archive restored')


if __name__ == '__main__':
    main()
