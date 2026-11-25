#!/usr/bin/env python3
"""Phase-2 static contract checks; they do NOT replace cargo test or Android QA."""
import json
from pathlib import Path
R = Path(__file__).resolve().parent.parent
V = R / 'rust/football_match_engine/src/v4'
assert (V/'own_goal.rs').is_file() and (V/'stats.rs').is_file()
assert 'pub mod own_goal;' in (V/'mod.rs').read_text()
assert 'pub mod stats;' in (V/'mod.rs').read_text()
base=(V.parent/'contracts/events.rs').read_text()
for required in ['pub own_goals: u8','pub own_goals_for: u16','pub own_goals_conceded: u16',
                 'pub shots_off_target: u16','pub shots_blocked: u16','pub saves: u16']:
 assert required in base, required
text=(V/'session.rs').read_text()
assert 'EventTypeV4::Goal | EventTypeV4::OwnGoal' in text
assert 'own_goals_for: self.state.home.own_goals_for' in text
assert 'own_goals_for: self.state.away.own_goals_for' in text
assert 'own_goals: p.own_goals' in text
assert 'kind: EventTypeV4::OwnGoal' in (V/'own_goal.rs').read_text()
assert 'state.team_state_mut(att_side).shots_blocked += 1' in (V/'shooting.rs').read_text()
assert 'state.team_state_mut(att_side).shots_off_target += 1' in (V/'shooting.rs').read_text()
assert 'state.team_state_mut(def_side).saves += 1' in (V/'set_pieces.rs').read_text()
assert 'pub fn audit_result(' in (V/'stats.rs').read_text()
assert 'pub fn validate_summary(' in (V/'stats.rs').read_text()
assert 'p2_fastbatch_summary_and_timeline_match_fullevent' in (V/'tests.rs').read_text()
assert 'p2_all_54_named_attributes_serialise_validate_and_change_input_fingerprint' in (V/'tests.rs').read_text()
assets=R/'app/src/main/assets/england_v2'
files = sorted(assets.glob('division_ENG_*.json'))
assert len(files) == 11, len(files)
players = 0
for path in files:
 raw = json.loads(path.read_text())
 players += len(raw['players'])
 assert len(raw['players']) >= len(raw['clubs']) * 24
 for player in raw['players']:
  assert sum(map(len, player['simulation']['attributes'].values())) == 54
assert players == 5640, players
print('P2 STATIC CONTRACT: PASS (54-attribute schemas, own-goals, match stats, parity hooks, 11 divisions)')
print('NOT A COMPILER TEST: cargo test and Android build still required.')
