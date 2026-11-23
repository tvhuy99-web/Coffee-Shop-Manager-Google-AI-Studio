#!/usr/bin/env python3
"""Read-only integration regression checks for V4 P1-2/P1-3/P1-4.

STATIC/STRUCTURAL checks, not a substitute for cargo tests or Android UI QA.
"""
from pathlib import Path
import json
import sys
ROOT = Path(__file__).resolve().parent.parent
R = ROOT / 'rust/football_match_engine/src/v4'
K = ROOT / 'app/src/main/java/pl/mkrstudio/truefootball3/lab'
checks = []
def test(name, condition):
    checks.append((name, bool(condition)))
def read(p):return p.read_text(encoding='utf-8')
a = read(R/'ai_manager.rs')
s = read(R/'session.rs')
e = read(R/'events.rs')
t = read(R/'tests.rs')
w = read(K/'LabRustLiveSession.kt')
ui = read(K/'LabReferenceMatchActivity.kt')
st = read(K/'LabGlobalSeedReplayStore.kt')
factory = read(K/'LabMatchInputFactory.kt')
main = read(K/'LabMainActivity.kt')
test('substitution group validated and atomically committed', 'let mut candidate = state.clone()' in a and '*state = candidate;' in a and 'duplicate or self-referential substitution' in a)
test('single in-play window charged for grouped changes', 'if !during_interval {' in a and 'candidate.team_state_mut(side).substitution_windows_used += 1;' in a)
test('half-time allows player change without consuming a window', 'fn is_interval_substitution(&self)' in s and 'Period::SecondHalf | Period::ExtraTimeFirst | Period::ExtraTimeSecond' in s)
test('V4 command can express simultaneous substitutions', 'SubstituteBatch' in s and '"SUBSTITUTE_BATCH"' in w and '"changes"' in w)
test('Rust roster state reports actual substitutes and counters', 'pub struct TeamRosterStateV4' in e and 'home_roster: self.roster_snapshot' in s and 'away_roster: self.roster_snapshot' in s)
test('commands emitted between steps are delivered', 'last_delivered_event_index' in s and 'let prev_len = self.last_delivered_event_index;' in s)
test('Android UI reconciles roster with Rust rather than local guesses only', 'syncV4HomeRoster' in ui and 'homeRosterIndex' in ui and 'homeWindowsUsed' in ui)
test('Android UI supports same-stoppage multi-person choice', 'setMultiChoiceItems' in ui and 'submitSubstitutions(' in ui)
test('V4 simulation runs at most one beat ahead', 'session.advance(maxSteps = if (engineVersion == LabEngineVersion.V4) 1 else 8)' in ui)
test('Android gates queued commands on presented beat', 'presentedV4Beat != simulatedV4Beat' in ui and 'pendingV4Commands.removeFirst()' in ui and 'played >= narration.size' in ui)
test('Android prevents continued presentation during command dialog', 'commandDialogActive' in ui and 'main.removeCallbacks(tick)' in ui)
test('unique V4 match UUID and optional immutable fixture ID', 'java.util.UUID.randomUUID()' in factory and 'matchInstanceId: String? = null' in factory)
test('snapshot filenames hash actual match ID', 'sha256(matchId.toByteArray' in st and '"match-$token.json"' in st)
test('snapshot writes use Android atomic transaction with integrity digest', 'AtomicFile(destination)' in st and 'atomic.startWrite()' in st and 'atomic.finishWrite(stream)' in st and 'atomic.failWrite(stream)' in st and 'recordDigest' in st)
test('snapshot never silently overwrites a different match', 'if (previous == record) return' in st and 'Immutable match archive already exists' in st)
test('snapshot validates input match ID, seed, engine version and fingerprint', 'input.getString("matchId") == record.matchId' in st and 'input.getLong("seed") == record.seed' in st and 'record.eventTimelineFingerprint.isNotBlank()' in st)
test('benchmark snapshot records the event timeline fingerprint', 'eventTimelineFingerprint = firstFast?.optString(' in main)
for name in ['p1_batch_substitution_counts_one_window_and_returns_authoritative_roster','p1_halftime_substitution_consumes_player_slot_not_inplay_window','p1_grouped_substitutions_replay_deterministically','p1_fourth_inplay_window_is_rejected_even_with_player_slots_left']:
    test('rust test present: '+name, name in t)
clubs = list((ROOT/'app/src/main/assets/england_v2').glob('division_*.json'))
test('11 existing England divisions unchanged and parseable', len(clubs)==11 and all(isinstance(json.loads(f.read_text(encoding='utf-8')),dict) for f in clubs))
for name,ok in checks: print(('PASS' if ok else 'FAIL'),name)
print(f'V4 P1-2/3/4 STATIC INTEGRATION: {sum(v for _,v in checks)}/{len(checks)}')
sys.exit(0 if all(x[1] for x in checks) else 1)
