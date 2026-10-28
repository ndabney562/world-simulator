#!/usr/bin/env python3
"""Measure the complete exported player, including exact artifact digests."""
import hashlib
import json
import sys
from pathlib import Path


def manifest(root):
    root = Path(root)
    if not (root / 'index.html').is_file():
        raise ValueError('Missing full-player index.html')
    files = [p for p in root.rglob('*') if p.is_file() and p.name != 'build-manifest.json']
    if not any('.wasm' in p.name for p in files):
        raise ValueError('Missing player WebAssembly module')
    rows = [{'path': p.relative_to(root).as_posix(), 'bytes': p.stat().st_size,
             'sha256': hashlib.sha256(p.read_bytes()).hexdigest()} for p in sorted(files)]
    return {'total_payload_bytes': sum(row['bytes'] for row in rows), 'files': rows}


if __name__ == '__main__':
    folder = Path(sys.argv[1])
    result = manifest(folder)
    (folder / 'build-manifest.json').write_text(json.dumps(result, indent=2) + '\n')
    print(f"Complete player payload: {result['total_payload_bytes']:,} bytes (excluding manifest)")
