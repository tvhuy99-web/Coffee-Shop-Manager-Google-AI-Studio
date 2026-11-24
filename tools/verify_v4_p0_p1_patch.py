#!/usr/bin/env python3
"""Static source-wire safety checks for V4 P0/P1 patch.
NOT a substitute for `cargo test` or an Android ARM64 device smoke test.
"""
from pathlib import Path
import json, re, sys
ROOT=Path(__file__).resolve().parent.parent
R=ROOT/'rust/football_match_engine/src'
K=ROOT/'app/src/main/java/pl/mkrstudio/truefootball3/lab'
checks=[]
def chk(name,assertion):
    checks.append((name, bool(assertion)))
def get(path):return Path(path).read_text(encoding='utf-8')
ball=get(R/'v4/ball_play.rs')
sess=get(R/'v4/session.rs')
jni=get(R/'jni_bridge.rs')
events=get(R/'v4/events.rs')
mod=get(R/'v4/mod.rs')
bridge=get(K/'NativeBridge.kt')
activity=get(K/'LabReferenceMatchActivity.kt')
store=get(K/'LabGlobalSeedReplayStore.kt')
wrapper=get(K/'LabRustLiveSession.kt')
chk('P0-1 offside snapshot + receiver involvement',
    'receiver_in_offside_position' in ball and
    ball.index('if receiver_in_offside_position {') > ball.index('// Misplaced pass that stayed in bounds:') and
    'caught_prob' not in ball and 'OFFSIDE_INVOLVEMENT' in ball)
chk('P0-2 no fabricated penalty goals', 'if round >= 20' not in sess and
    'shootout_home_goals += 1;\n                break' not in sess and
    'shootout_eligible_order' in sess and 'MAX_ROUNDS' in sess)
section=jni[jni.index('// Match Engine V4.0 Interactive JNI.') :]
chk('P0-3 V4 handles are NOT raw pointers',
    'OnceLock<Mutex<HashMap<i64, V4SessionArc>>>' in section and
    'as *mut v4::MatchSessionV4' not in section and 'Box::from_raw' not in section and
    'registry.remove(&handle)' in section)
chk('P0-4 atomic authoritative kickoff restart',
    sess.count('restart_after_goal(def_side)')==4 and
    'self.state.ball.vel = Vec3::default();' in sess and
    'self.state.ball.owner_player_id = Some(kicker_id.clone());' in sess and
    'self.state.ball.last_touch_player_id = None;' in sess)
chk('P0-5 command journal emitted by Rust',
    'pub struct AppliedCommandV4' in sess and
    sess.count('self.command_timeline.push(')==2 and
    'replay_v4_with_timeline' in sess and 'command_timeline:' in events)
chk('P0-5 JNI replay boundary', 'nativeReplayV4Json' in jni and
    'pub fn replay_v4_with_timeline' in sess and 'nativeReplayV4Json' in bridge)
chk('P0-5 Android snapshot uses Rust timeline and fingerprint',
    'commandTimelineJson = finalObj.optJSONArray("commandTimeline")' in activity and
    'eventTimelineFingerprint = finalObj.optString' in activity and
    'Replay fingerprint mismatch' in store)
chk('P1-1 period elapsed passed from Rust to Kotlin',
    'pub period_elapsed_second: i32' in events and
    'period_elapsed_second: g_sec - self.state.clock.period_started_at_second' in sess and
    'ev.optInt("periodElapsedSecond"' in activity and
    'formatDisplayMinute(period, evSec, periodElapsed)' in activity)
chk('JNI bridge version bumped with new replay API',
    'JNI_BRIDGE_VERSION: i32 = 7' in mod and
    'nativeBridgeVersion() >= 7' in bridge)
chk('Android lifecycle serializes close versus active JNI call',
    wrapper.count('@Synchronized')>=5 and '@Volatile private var handle' in wrapper)
chk('Regression tests cover key P0/P1 cases',
    all(s in get(R/'v4/tests.rs') for s in [
        'p0_offside_position_is_not_automatically_an_offence',
        'p0_controlled_game_replays_identical_commands_and_fingerprint',
        'p0_shootout_scores_come_only_from_actual_kicks',
        'p1_regulation_clock_uses_independent_period_elapsed_seconds',
        'p0_every_goal_is_followed_by_opponent_kickoff_at_centre']))
club_files=sorted((ROOT/'app/src/main/assets/england_v2').glob('division_*.json'))
chk('All 11 England-v2 data files still parse',
    len(club_files)==11 and all(isinstance(json.loads(p.read_text()),dict) for p in club_files))
for name,passed in checks:print(('PASS' if passed else 'FAIL'),name)
print(f'STATIC CHECKS: {sum(ok for _,ok in checks)}/{len(checks)} PASS; not a compiler/runtime test')
sys.exit(0 if all(ok for _,ok in checks) else 1)
