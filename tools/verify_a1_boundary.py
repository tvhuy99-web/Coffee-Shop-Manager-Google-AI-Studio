#!/usr/bin/env python3
"""Reject obsolete production paths and verify historical fixture identity."""
import hashlib,json,re,subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
REFERENCE='67e319fd844b745100ff38c224410f61e7f382cc'
def reference_bytes(path):
    return subprocess.check_output(['git','show',f'{REFERENCE}:{path}'],cwd=ROOT)
manifest=json.loads(reference_bytes('docs/archive/legacy/manifest.json'))
rows=[]
for row in manifest['files']:
    data=reference_bytes(row['archive'])
    blob=hashlib.sha1(f'blob {len(data)}\0'.encode()+data).hexdigest()
    assert blob==row['git_blob'], row['archive']
    assert not (ROOT/row['original']).exists(), row['original']
    if row['archive'].startswith('docs/archive/legacy/tests/parity/') and row['archive'].endswith(('.csv','.json')):
        current=ROOT/'tests/fixtures/legacy'/Path(row['archive']).name
        assert current.read_bytes()==data, current
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
print(f'Historical identity and entrypoint gate: {len(rows)} archived files, 0 active V3 JNI entrypoints')
