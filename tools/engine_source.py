"""Read split Engine implementations for existing source-contract gates."""
from pathlib import Path
import re

def session_source(v4: Path) -> str:
    source = (v4 / 'session.rs').read_text(encoding='utf-8')
    def expand(match):
        return (v4 / match.group(1)).read_text(encoding='utf-8')
    return re.sub(r'include!\("(session_parts/[^"]+\.rs)"\);', expand, source)

def engine_test_source(v4: Path) -> str:
    return (v4.parents[2] / 'tests/engine/v4_tests.rs').read_text(encoding='utf-8')
