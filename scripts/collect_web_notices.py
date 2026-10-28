#!/usr/bin/env python3
"""Collect local licence sources for release review; do not infer redistribution rights."""
import json
import subprocess
import shutil
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def collect(output):
    output = Path(output)
    sections = ['World Simulator desktop browser preview\n\nOriginal code: MIT (LICENSE).\n'
                'Third-party software and data retain their own terms. This inventory includes build-time packages; '
                'it is not a declaration that Unity or datasets are MIT licensed.\n\n'
                'Building footprints: NYC Office of Technology and Innovation, NYC Open Data.\n'
                'Tax lot data: PLUTO, NYC Department of City Planning, NYC Open Data.\n'
                'Dataset versions: footprints snapshot 2026-10-18; PLUTO 26v2. No endorsement implied.\n']
    sections.append((ROOT / 'LICENSE').read_text())
    meta = json.loads(subprocess.check_output(['cargo', '+1.97.0', 'metadata', '--manifest-path',
                      str(ROOT / 'core/Cargo.toml'), '--locked', '--offline', '--format-version', '1'], text=True))
    for package in sorted(meta['packages'], key=lambda p: (p['name'], p['version'])):
        if not package.get('source'):
            continue
        folder = Path(package['manifest_path']).parent
        sections.append(f"\n--- Rust package: {package['name']} {package['version']} ({package.get('license')}) ---\n")
        found = []
        for pattern in ('LICENSE*', 'COPYING*', 'NOTICE*', 'UNLICENSE*'):
            found += [p for p in folder.glob(pattern) if p.is_file()]
        if not found:
            sections.append('No top-level license text collected; inspect this package before release.\n')
        for path in sorted(set(found)):
            sections.append(path.name + '\n' + path.read_text(errors='replace'))
    rust_root = Path(subprocess.check_output(['rustc', '+1.97.0', '--print', 'sysroot'], text=True).strip())
    copyright_file = rust_root / 'share/doc/rust/COPYRIGHT-library.html'
    if not copyright_file.is_file():
        raise RuntimeError('Missing Rust standard-library copyright inventory')
    (output / 'RUST_LIBRARY_COPYRIGHT.html').write_bytes(copyright_file.read_bytes())
    # Include all locally resolved Unity package notices conservatively, including editor-only tools.
    packages = ROOT / 'apps/unity/Library/PackageCache'
    if not packages.is_dir():
        raise RuntimeError('Run the Unity import/export before collecting package notices')
    for folder in sorted(packages.iterdir()):
        if not folder.is_dir():
            continue
        for name in ('LICENSE.md', 'LICENSE.txt', 'Third Party Notices.md', 'ThirdPartyNotices.txt'):
            path = folder / name
            if path.is_file():
                sections.append('\n--- Unity package: ' + folder.name + '/' + name + ' ---\n' + path.read_text(errors='replace'))
    shutil.copyfile(ROOT / 'docs/legal/unity-6000.3.26f1-webgl-notices.pdf', output / 'UNITY_PLAYER_NOTICES.pdf')
    sections.append('\nUnity engine third-party notices: UNITY_PLAYER_NOTICES.pdf\nRust standard-library copyright details: RUST_LIBRARY_COPYRIGHT.html\n'
                    'Release gate: review Unity Player/engine redistribution notices and selected Editor terms separately; '
                    'package notices alone do not cover the engine.\n')
    (output / 'THIRD_PARTY_NOTICES.txt').write_text('\n'.join(sections))


if __name__ == '__main__':
    collect(ROOT / 'apps/unity/Builds/DesktopWeb')
