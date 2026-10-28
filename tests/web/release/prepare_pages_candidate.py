"""Stage exactly the reviewed player for a future approved static publication.

This writes a NEW local directory only. It never uploads, pushes, configures Pages,
or copies test probes, profiles or saves. Re-run with a different output directory.
"""
import hashlib
import json
from pathlib import Path, PurePosixPath
import shutil
import sys

root=Path(__file__).resolve().parents[3]
source=root/'apps/unity/Builds/DesktopWeb'
out=Path(sys.argv[1]).resolve()
assert not out.exists(),'Candidate directory already exists; choose a new path'
manifest_path=source/'build-manifest.json'
# This staging tool is bound to the candidate whose complete checks are recorded
# in issue112. A new export needs a new reviewed artifact identity.
expected_manifest='9e24465401907ba6683687bbe86044f2ea5bdfbd72cf31178ea031066d4cb508'
assert hashlib.sha256(manifest_path.read_bytes()).hexdigest()==expected_manifest,'Unreviewed export manifest'
manifest=json.loads(manifest_path.read_text())
required={'index.html','player.js','THIRD_PARTY_NOTICES.txt','UNITY_PLAYER_NOTICES.pdf','RUST_LIBRARY_COPYRIGHT.html'}
paths=set()
for row in manifest['files']:
    name=PurePosixPath(row['path'])
    assert not name.is_absolute() and '..' not in name.parts
    assert row['path'] not in paths
    paths.add(row['path'])
    file=(source/row['path']).resolve()
    assert source.resolve() in file.parents
    data=file.read_bytes()
    assert len(data)==row['bytes'] and hashlib.sha256(data).hexdigest()==row['sha256']
assert required <= paths
assert any(p.endswith('.wasm.unityweb') for p in paths)
assert sum(row['bytes'] for row in manifest['files'])==manifest['total_payload_bytes']
# Marker checks distinguish the real release payload from the instrumented panic player.
import gzip
assert all(b'WORLD_SIM_RELEASE_PROBE_RUST_PANIC' not in gzip.decompress((source/p).read_bytes()) for p in paths if p.endswith('.wasm.unityweb'))
out.mkdir(parents=True)
for name in paths:
    target=out/name;target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(source/name,target)
shutil.copy2(manifest_path,out/'build-manifest.json')
(out/'.nojekyll').write_bytes(b'')
info={'runtime_commit':'8de9a2cb393383a61d2e66ca3ce4d1441a2de424',
      'runtime_tree':'c02ea2cb27d9b4496bf31b96f80b230e81c9db17',
      'manifest_sha256':hashlib.sha256(manifest_path.read_bytes()).hexdigest(),
      'payload_bytes':manifest['total_payload_bytes'],
      'artifact_role':'verified local publication candidate'}
(out/'publication-info.json').write_text(json.dumps(info,indent=2)+'\n')
print(json.dumps(info))
