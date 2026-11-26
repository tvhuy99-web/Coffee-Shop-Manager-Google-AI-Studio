#!/usr/bin/env python3
"""Verify the workspace move does not change any external dependency identity."""
import sys,tomllib,hashlib
from pathlib import Path
base,candidate=map(Path,sys.argv[1:3])
expected='41d521b51131a20781790cb9ee4c62c967f34003866d50d97df2a3354eae9099'
assert hashlib.sha256(base.read_bytes()).hexdigest()==expected
def packages(p):
    return tomllib.loads(p.read_text())['package']
old,new=packages(base),packages(candidate)
external=lambda rows: sorted((r for r in rows if 'source' in r),key=lambda r:(r['name'],r['version'],r['source']))
assert external(old)==external(new), 'registry graph changed'
assert {r['name'] for r in new if 'source' not in r}=={'match_engine','football_match_engine_jni','match_contracts','player_intelligence','coach_intelligence','match_physics','player_condition','match_tactics','match_rules','match_telemetry'}
print('PASS frozen external Cargo graph, workspace has 10 real local packages')
