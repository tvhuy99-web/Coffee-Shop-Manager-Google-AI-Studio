#!/usr/bin/env python3
"""Static source-wire safety checks for V4 P0/P1 patch.
NOT a substitute for `cargo test` or an Android ARM64 device smoke test.
"""
from pathlib import Path
from engine_source import session_source, engine_test_source
import json, re, sys
ROOT=Path(__file__).resolve().parent.parent
R=ROOT/'rust/football_match_engine/src'
K=ROOT/'app/src/main/java/pl/mkrstudio/truefootball3/lab'
checks=[]
def chk(name,assertion):
    checks.append((name, bool(assertion)))
def get(path):return Path(path).read_text(encoding='utf-8')
def ordered(source, *tokens):
    """Missing anchors fail a check rather than raising ValueError."""
    cursor = 0
    for token in tokens:
        pos = source.find(token, cursor)
        if pos < 0:
            return False
        cursor = pos + len(token)
    return True
def between(source, start, end):
    begin = source.find(start)
    if begin < 0:
        return ''
    stop = source.find(end, begin + len(start))
    return source[begin:stop] if stop >= 0 else ''
ball=get(R/'v4/ball_play.rs')
sess=session_source(R/'v4')
commands=get(R/'v4/session_commands.rs')
replay=get(R/'v4/session_replay.rs')
jni=get(ROOT/'rust/crates/football_match_engine_jni/src/lib.rs')
events=get(R/'contracts/events.rs')+'\n'+get(ROOT/'rust/crates/match_contracts/src/event_types.rs')
mod=get(R/'v4/mod.rs')
bridge=get(K/'NativeBridge.kt')
activity=get(K/'LabReferenceMatchActivity.kt')
cue_adapter=get(K/'match_commentary/MatchEventCueAdapter.kt')
store=get(K/'match_playback/LabGlobalSeedReplayStore.kt')
wrapper=get(K/'match_playback/LabRustLiveSession.kt')
chk('P0-1 offside snapshot + receiver involvement',
    ordered(ball, 'let receiver_in_offside_position =',
            'if receiver_contact.is_none() {',
            'receiver_contact.expect("reachable receiver")',
            'if receiver_in_offside_position {', 'OFFSIDE_INVOLVEMENT') and
    ordered(sess, 'fn finish_pass_recovery(', 'offside_at_launch.contains(&id)',
            'EventTypeV4::Offside', 'DeadBallKind::Offside') and
    'caught_prob' not in ball)
chk('P0-2 no fabricated penalty goals', 'if round >= 20' not in sess and
    'shootout_home_goals += 1;\n                break' not in sess and
    'shootout_eligible_order' in sess and
    ordered(sess, 'if round >= MAX_ROUNDS {', 'return Err(V4Error::InvalidInput('))
# Scope registry checks to the V4 DES implementation; legacy V3 uses
# a different ABI. Require an actual V4 marker instead of slicing at -1.
marker = re.search(r'// Match Engine V4\.\d+ (?:DES )?Interactive JNI\.', jni)
section = jni[marker.start():] if marker else ''
chk('P0-3 V4 handles are NOT raw pointers',
    'OnceLock<Mutex<HashMap<i64, V4SessionArc>>>' in section and
    'as *mut v4::MatchSessionV4' not in section and 'Box::from_raw' not in section and
    'registry.remove(&handle)' in section)
kickoff = between(sess, 'fn restart_after_goal_kickoff(', 'fn reset_kickoff_positions(')
chk('P0-4 atomic authoritative kickoff restart',
    ordered(sess, 'DeadBallKind::GoalCelebration | DeadBallKind::Kickoff => {',
            'self.restart_after_goal_kickoff(restart_side)', 'self.schedule_next_decision(') and
    ordered(kickoff, 'self.reset_kickoff_positions(kickoff_side);',
            'self.state.ball.pos = Vec3::center_spot();',
            'self.state.ball.vel = Vec3::default();',
            'self.state.ball.control = BallControlState::Controlled;',
            'self.state.ball.owner_side = Some(kickoff_side);',
            'self.state.ball.owner_player_id = Some(kicker_id.clone());',
            'self.state.ball.last_touch_side = None;',
            'self.state.ball.last_touch_player_id = None;',
            'self.possession_side = kickoff_side;', 'self.carrier_idx = kicker;',
            'EventTypeV4::Kickoff'))
chk('P0-5 command journal emitted by Rust',
    'pub struct AppliedCommandV4' in commands and
    all('status: CommandLifecycleStatus::'+status in sess
        for status in ('Applied','Scheduled','QueuedForDeadBall')) and
    all(field in sess for field in ('accepted_sim_time_ms:',
        'scheduled_effective_sim_time_ms:', 'actual_effective_sim_time_ms:')) and
    'self.command_timeline.push(AppliedCommandV4 {' in sess and
    'command_timeline: self.command_timeline.clone()' in sess and
    'replay_v4_with_timeline' in sess and 'command_timeline:' in events)
chk('P0-5 JNI replay boundary', 'nativeReplayV4Json' in jni and
    'pub fn replay_v4_with_timeline' in replay and 'nativeReplayV4Json' in bridge)
chk('P0-5 Android snapshot uses Rust timeline and fingerprint',
    'commandTimelineJson = finalObj.optJSONArray("commandTimeline")' in activity and
    'eventTimelineFingerprint = finalObj.optString' in activity and
    'Replay fingerprint mismatch' in store)
chk('P1-1 period elapsed passed from Rust to Kotlin',
    'pub period_elapsed_second: i32' in events and
    ordered(sess, 'let p_sec = self.state.des_clock.period_elapsed_second();',
            'period_elapsed_second: p_sec,') and
    'cueAdapter.convertV4StepEvents(stepObj, rawEvents, filteredEvents,' in activity and
    'ev.optInt("periodElapsedSecond"' in cue_adapter and
    'formatDisplayMinute(period, evSec, periodElapsedSec)' in cue_adapter)
version = re.search(r'JNI_BRIDGE_VERSION:\s*i32\s*=\s*(\d+)', mod)
chk('JNI bridge supports replay and DES protocol',
    version is not None and int(version.group(1)) >= 8 and
    'nativeBridgeVersion() >= 7' in bridge and
    'nativeBridgeVersion() >= 8 && nativeEngineProtocolVersion() >= 5' in bridge)
chk('Android lifecycle serializes close versus active JNI call',
    wrapper.count('@Synchronized')>=5 and '@Volatile private var handle' in wrapper)
chk('Regression tests cover key P0/P1 cases',
    all(s in engine_test_source(R/'v4') for s in [
        'p0_offside_position_is_not_automatically_an_offence',
        'p0_controlled_game_replays_identical_commands_and_fingerprint',
        'p0_shootout_scores_come_only_from_actual_kicks',
        'p1_regulation_clock_uses_independent_period_elapsed_seconds',
        'p0_every_goal_is_followed_by_opponent_kickoff_at_centre',
        'des_goal_celebration_then_kickoff_nonzero_variable_duration',
        'des_tactical_command_delayed_effective_time_and_override_policy',
        'des_checkpoint_roundtrip_exact_continuation_all_states',
        'qa10_mid_interval_tactical_event_preserves_total_movement_and_fatigue']))
club_files=sorted((ROOT/'app/src/main/assets/england_v2').glob('division_*.json'))
chk('All 11 England-v2 data files still parse',
    len(club_files)==11 and all(isinstance(json.loads(p.read_text()),dict) for p in club_files))
for name,passed in checks:print(('PASS' if passed else 'FAIL'),name)
print(f'STATIC CHECKS: {sum(ok for _,ok in checks)}/{len(checks)} PASS; not a compiler/runtime test')
sys.exit(0 if all(ok for _,ok in checks) else 1)
