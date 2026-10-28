#!/usr/bin/env python3
"""Run the pinned Editor and restore build-only serialized settings on exit."""
import os
import subprocess
import sys
from pathlib import Path


def run(editor, project, log):
    project = Path(project)
    files = list((project / 'ProjectSettings').glob('*'))
    files += list((project / 'Assets').rglob('*.asset'))
    files += list((project / 'Packages').glob('*.json'))
    original = {p: p.read_bytes() for p in files if p.is_file()}
    try:
        return subprocess.call([str(editor), '-batchmode', '-nographics', '-projectPath', str(project),
                                '-buildTarget', 'WebGL', '-executeMethod', 'WorldSim.EditorTools.WebBuild.Build',
                                '-quit', '-logFile', str(log)])
    finally:
        for path, content in original.items():
            if not path.exists() or path.read_bytes() != content:
                path.write_bytes(content)


if __name__ == '__main__':
    root = Path(__file__).resolve().parent.parent
    editor_root = Path(os.environ.get('WORLD_SIM_UNITY_ROOT', '/Applications/Unity/Hub/Editor/6000.3.26f1'))
    sys.exit(run(editor_root / 'Unity.app/Contents/MacOS/Unity', root / 'apps/unity',
                 root / 'core/target/desktop-web-build.log'))
