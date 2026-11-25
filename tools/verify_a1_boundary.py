#!/usr/bin/env python3
"""Reject legacy production paths and verify archive identity."""
import hashlib,json,re
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
manifest=json.loads((ROOT/'docs/archive/legacy/manifest.json').read_text())
rows=[]
for row in manifest['files']:
    data=(ROOT/row['archive']).read_bytes()
    blob=hashlib.sha1(f'blob {len(data)}\0'.encode()+data).hexdigest()
    assert blob==row['git_blob'], row['archive']
    assert not (ROOT/row['original']).exists(), row['original']
    rows.append({'path':row['archive'],'sha256':hashlib.sha256(data).hexdigest(),'size':len(data)})
r=ROOT/'rust/football_match_engine/src'
assert {p.name for p in r.glob('*.rs')}=={'lib.rs','clock.rs','rng.rs','fingerprints.rs'}
legacy=['nativePortVersion','rngSample','nativeCoreSelfCheck','nativeSimulateMatchJson','nativeCreateSession','nativeSubmitCommand','nativeAdvanceSession','nativeFinishSession','nativeDestroySession']
for p in (ROOT/'app/src/main/java').rglob('*.kt'):
    s=p.read_text()
    assert not re.search(r'LabEngineVersion\.V3\b|\bproject54ForV3\b',s),p
    for name in legacy: assert not re.search(r'\b'+name+r'\b',s),(p,name)
for p in [r/'lib.rs',ROOT/'rust/crates/football_match_engine_jni/src/lib.rs']:
    s=p.read_text()
    for name in legacy: assert not re.search(r'NativeBridge_'+name+r'\b',s),(p,name)
out=ROOT/'evidence';out.mkdir(exist_ok=True)
(out/'legacy-sha256.json').write_text(json.dumps(rows,indent=2)+'\n')
print(f'A1 archive and entrypoint gate: {len(rows)} archived files, 0 active V3 JNI entrypoints')
