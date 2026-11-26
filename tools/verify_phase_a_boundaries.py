#!/usr/bin/env python3
"""Phase A production policy import boundary; no behavioral changes."""
from pathlib import Path
import re
root=Path(__file__).resolve().parents[1]
for crate in ('player_intelligence','coach_intelligence'):
    paths=sorted((root/'rust/crates'/crate/'src').rglob('*.rs'))
    assert paths,crate
    for path in paths:
        source=path.read_text(encoding='utf-8')
        # Strip line comments for the token check; imports in code are forbidden.
        code=re.sub(r'//[^\n]*','',source)
        assert not re.search(r'\b(?:MatchStateV4|MatchInputV4|DeterministicRng)\b|\b(?:match_engine|football_match_engine|v4)::',code),path
    print('PASS Phase A policy boundary:',crate,len(paths),'files')
