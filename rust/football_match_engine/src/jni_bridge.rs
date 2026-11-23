use std::ffi::{c_char, c_void, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::sync::atomic::{AtomicI64, Ordering};

use jni::objects::{JClass, JString as SafeJString};
use jni::sys::jstring;
use jni::JNIEnv;

use crate::{
    discipline,
    geometry::TeamSide,
    session::{MatchSession, UnifiedMatchEngine},
    v4,
    wire_json,
};

type JniEnv = *mut *const *const c_void;
type JString = *mut c_void;

const JNI_NEW_STRING_UTF: usize = 167;
const JNI_GET_STRING_UTF_CHARS: usize = 169;
const JNI_RELEASE_STRING_UTF_CHARS: usize = 170;

unsafe fn read_jstring(env: JniEnv, jstr: JString) -> Result<String, &'static str> {
    if env.is_null() || jstr.is_null() {
        return Err("null JNIEnv or jstring");
    }
    let vtable = *env;
    let get_fn: extern "system" fn(JniEnv, JString, *mut u8) -> *const c_char =
        std::mem::transmute(*vtable.add(JNI_GET_STRING_UTF_CHARS));
    let rel_fn: extern "system" fn(JniEnv, JString, *const c_char) =
        std::mem::transmute(*vtable.add(JNI_RELEASE_STRING_UTF_CHARS));
    let raw = get_fn(env, jstr, std::ptr::null_mut());
    if raw.is_null() {
        return Err("GetStringUTFChars returned null");
    }
    let rust_str = CStr::from_ptr(raw).to_string_lossy().into_owned();
    rel_fn(env, jstr, raw);
    Ok(rust_str)
}

unsafe fn make_jstring(env: JniEnv, text: &str) -> JString {
    if env.is_null() {
        return std::ptr::null_mut();
    }
    let vtable = *env;
    let new_fn: extern "system" fn(JniEnv, *const c_char) -> JString =
        std::mem::transmute(*vtable.add(JNI_NEW_STRING_UTF));
    match CString::new(text) {
        Ok(cstr) => new_fn(env, cstr.as_ptr()),
        Err(_) => {
            let sanitized = text.replace('\0', " ");
            let fallback = CString::new(sanitized).unwrap_or_else(|_| CString::new("{\"ok\":false}").unwrap());
            new_fn(env, fallback.as_ptr())
        }
    }
}

#[no_mangle]
pub unsafe extern "system" fn Java_pl_mkrstudio_truefootball3_lab_NativeBridge_nativeSimulateMatchJson(
    env: JniEnv,
    _class: *mut c_void,
    input_json: JString,
) -> JString {
    let response = catch_unwind(AssertUnwindSafe(|| {
        let raw = match read_jstring(env, input_json) {
            Ok(s) => s,
            Err(e) => return wire_json::serialize_error(e),
        };
        let ai_cfg = wire_json::parse_ai_config(&raw);
        let input = match wire_json::parse_match_input(&raw) {
            Ok(i) => i,
            Err(e) => return wire_json::serialize_error(&e),
        };
        match UnifiedMatchEngine::new_session(input) {
            Ok(mut session) => {
                session.set_ai_manager(ai_cfg);
                match session.finish() {
                    Ok(result) => wire_json::serialize_match_result_with_ratings(session.input(), &result),
                    Err(e) => wire_json::serialize_error(e),
                }
            }
            Err(e) => wire_json::serialize_error(e),
        }
    }))
    .unwrap_or_else(|_| wire_json::serialize_error("Rust panic inside nativeSimulateMatchJson"));
    make_jstring(env, &response)
}

#[no_mangle]
pub unsafe extern "system" fn Java_pl_mkrstudio_truefootball3_lab_NativeBridge_nativeCreateSession(
    env: JniEnv,
    _class: *mut c_void,
    input_json: JString,
) -> i64 {
    catch_unwind(AssertUnwindSafe(|| {
        let raw = match read_jstring(env, input_json) {
            Ok(s) => s,
            Err(_) => return 0,
        };
        let ai_cfg = wire_json::parse_ai_config(&raw);
        let input = match wire_json::parse_match_input(&raw) {
            Ok(i) => i,
            Err(_) => return 0,
        };
        match UnifiedMatchEngine::new_session(input) {
            Ok(mut session) => {
                session.set_ai_manager(ai_cfg);
                Box::into_raw(Box::new(session)) as i64
            }
            Err(_) => 0,
        }
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "system" fn Java_pl_mkrstudio_truefootball3_lab_NativeBridge_nativeSubmitCommand(
    env: JniEnv,
    _class: *mut c_void,
    handle: i64,
    command_json: JString,
) -> i32 {
    if handle == 0 {
        return 0;
    }
    catch_unwind(AssertUnwindSafe(|| {
        let session = &mut *(handle as *mut MatchSession);
        let raw = match read_jstring(env, command_json) {
            Ok(s) => s,
            Err(_) => return 0,
        };
        let cmd = match wire_json::parse_match_command(&raw) {
            Ok(c) => c,
            Err(_) => return 0,
        };
        if session.submit_command(cmd).is_ok() { 1 } else { 0 }
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "system" fn Java_pl_mkrstudio_truefootball3_lab_NativeBridge_nativeAdvanceSession(
    env: JniEnv,
    _class: *mut c_void,
    handle: i64,
    max_steps: i32,
) -> JString {
    let response = catch_unwind(AssertUnwindSafe(|| {
        if handle == 0 {
            return wire_json::serialize_error("invalid session handle");
        }
        let session = &mut *(handle as *mut MatchSession);
        let prev_len = session.events().len();
        let steps = max_steps.clamp(1, 500);
        for _ in 0..steps {
            if session.is_full_time() {
                break;
            }
            session.advance();
            if session.is_full_time() {
                break;
            }
            let st = session.snapshot();
            let input_clone = crate::resolver_support::MatchRuntime::input(session).clone();
            if let Some(cmd) = discipline::resolve_goalkeeper_continuity(&input_clone, &st, TeamSide::Home) {
                let _ = session.submit_command(cmd);
            }
            if let Some(cmd) = discipline::resolve_goalkeeper_continuity(&input_clone, &st, TeamSide::Away) {
                let _ = session.submit_command(cmd);
            }
        }
        let new_events = if session.events().len() >= prev_len {
            &session.events()[prev_len..]
        } else {
            &[]
        };
        wire_json::serialize_session_step(session, new_events)
    }))
    .unwrap_or_else(|_| wire_json::serialize_error("Rust panic inside nativeAdvanceSession"));
    make_jstring(env, &response)
}

#[no_mangle]
pub unsafe extern "system" fn Java_pl_mkrstudio_truefootball3_lab_NativeBridge_nativeFinishSession(
    env: JniEnv,
    _class: *mut c_void,
    handle: i64,
) -> JString {
    let response = catch_unwind(AssertUnwindSafe(|| {
        if handle == 0 {
            return wire_json::serialize_error("invalid session handle");
        }
        let session = &mut *(handle as *mut MatchSession);
        match session.finish() {
            Ok(result) => wire_json::serialize_match_result_with_ratings(session.input(), &result),
            Err(e) => wire_json::serialize_error(e),
        }
    }))
    .unwrap_or_else(|_| wire_json::serialize_error("Rust panic inside nativeFinishSession"));
    make_jstring(env, &response)
}

#[no_mangle]
pub unsafe extern "system" fn Java_pl_mkrstudio_truefootball3_lab_NativeBridge_nativeDestroySession(
    _env: JniEnv,
    _class: *mut c_void,
    handle: i64,
) {
    if handle != 0 {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            drop(Box::from_raw(handle as *mut MatchSession));
        }));
    }
}

// ============================================================================
// Match Engine V4.0 Safe JNI Entrypoints (jni crate + serde_json + rayon)
// ============================================================================

fn v4_error_json(message: &str) -> String {
    serde_json::json!({
        "ok": false,
        "protocolVersion": v4::ENGINE_PROTOCOL_VERSION_V4,
        "engineVersion": v4::ENGINE_VERSION_V4,
        "error": message,
    })
    .to_string()
}

/// Simulates a single 90-minute V4 match (`FullEvent` or `FastBatch`) from JSON.
#[no_mangle]
pub extern "system" fn Java_pl_mkrstudio_truefootball3_lab_NativeBridge_nativeSimulateMatchV4Json<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    input_json: SafeJString<'local>,
) -> jstring {
    let response = catch_unwind(AssertUnwindSafe(|| {
        let raw: String = match env.get_string(&input_json) {
            Ok(s) => s.to_string_lossy().into_owned(),
            Err(e) => return v4_error_json(&format!("JNI string read error: {e}")),
        };
        let input: v4::MatchInputV4 = match serde_json::from_str(&raw) {
            Ok(i) => i,
            Err(e) => return v4_error_json(&format!("V4 JSON parse error: {e}")),
        };
        match v4::simulate_v4(input) {
            Ok(result) => serde_json::to_string(&result)
                .unwrap_or_else(|e| v4_error_json(&format!("V4 JSON serialize error: {e}"))),
            Err(e) => v4_error_json(&e.to_string()),
        }
    }))
    .unwrap_or_else(|_| v4_error_json("Rust panic inside nativeSimulateMatchV4Json"));

    env.new_string(&response)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

/// Simulates a JSON array of V4 matches in parallel using Rayon in `FastBatch` mode.
#[no_mangle]
pub extern "system" fn Java_pl_mkrstudio_truefootball3_lab_NativeBridge_nativeSimulateBatchV4Json<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    inputs_json: SafeJString<'local>,
) -> jstring {
    let response = catch_unwind(AssertUnwindSafe(|| {
        let raw: String = match env.get_string(&inputs_json) {
            Ok(s) => s.to_string_lossy().into_owned(),
            Err(e) => return v4_error_json(&format!("JNI string read error: {e}")),
        };
        let inputs: Vec<v4::MatchInputV4> = match serde_json::from_str(&raw) {
            Ok(list) => list,
            Err(e) => return v4_error_json(&format!("V4 batch JSON parse error: {e}")),
        };
        let total = inputs.len();
        let outcomes = v4::simulate_batch_fast_v4(inputs);
        let mut succeeded = 0usize;
        let mut failed = 0usize;
        let mut results = Vec::with_capacity(total);
        for outcome in outcomes {
            match outcome {
                Ok(res) => {
                    succeeded += 1;
                    results.push(
                        serde_json::to_value(res).unwrap_or_else(|e| {
                            serde_json::json!({"ok": false, "error": e.to_string()})
                        }),
                    );
                }
                Err(e) => {
                    failed += 1;
                    results.push(serde_json::json!({
                        "ok": false,
                        "error": e.to_string()
                    }));
                }
            }
        }
        serde_json::json!({
            "ok": failed == 0,
            "protocolVersion": v4::ENGINE_PROTOCOL_VERSION_V4,
            "engineVersion": v4::ENGINE_VERSION_V4,
            "total": total,
            "succeeded": succeeded,
            "failed": failed,
            "results": results,
        })
        .to_string()
    }))
    .unwrap_or_else(|_| v4_error_json("Rust panic inside nativeSimulateBatchV4Json"));

    env.new_string(&response)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

// ============================================================================
// Match Engine V4.0 Interactive JNI. Handles are opaque, monotonic IDs, NEVER
// memory addresses. Removal from the registry is safe during a concurrent
// advance because callers hold Arc references and a per-session mutex.
// ============================================================================

type V4SessionArc = Arc<Mutex<v4::MatchSessionV4>>;
static V4_SESSIONS: OnceLock<Mutex<HashMap<i64, V4SessionArc>>> = OnceLock::new();
static NEXT_V4_SESSION_ID: AtomicI64 = AtomicI64::new(1);

fn v4_registry() -> &'static Mutex<HashMap<i64, V4SessionArc>> {
    V4_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn v4_lookup_session(handle: i64) -> Result<V4SessionArc, String> {
    if handle <= 0 { return Err("invalid V4 session ID".into()); }
    let map = v4_registry().lock().map_err(|_| "V4 session registry poisoned")?;
    map.get(&handle).cloned().ok_or_else(|| "V4 session already closed or unknown".into())
}

#[no_mangle]
pub extern "system" fn Java_pl_mkrstudio_truefootball3_lab_NativeBridge_nativeCreateV4Session<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    input_json: SafeJString<'local>,
) -> i64 {
    catch_unwind(AssertUnwindSafe(|| {
        let raw: String = match env.get_string(&input_json) {
            Ok(s) => s.to_string_lossy().into_owned(),
            Err(_) => return 0,
        };
        let input: v4::MatchInputV4 = match serde_json::from_str(&raw) {
            Ok(i) => i,
            Err(_) => return 0,
        };
        let session = match v4::MatchSessionV4::new(input) {
            Ok(s) => s,
            Err(_) => return 0,
        };
        let id = NEXT_V4_SESSION_ID.fetch_add(1, Ordering::Relaxed);
        // If an id ever wraps, refuse the session instead of risking reuse.
        if id <= 0 { return 0; }
        match v4_registry().lock() {
            Ok(mut registry) => {
                registry.insert(id, Arc::new(Mutex::new(session)));
                id
            }
            Err(_) => 0,
        }
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub extern "system" fn Java_pl_mkrstudio_truefootball3_lab_NativeBridge_nativeSubmitV4Command<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: i64,
    command_json: SafeJString<'local>,
) -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        let raw: String = match env.get_string(&command_json) {
            Ok(s) => s.to_string_lossy().into_owned(),
            Err(_) => return 0,
        };
        let command: v4::MatchCommandV4 = match serde_json::from_str(&raw) {
            Ok(c) => c,
            Err(_) => return 0,
        };
        let entry = match v4_lookup_session(handle) { Ok(e) => e, Err(_) => return 0 };
        let mut session = match entry.lock() { Ok(s) => s, Err(_) => return 0 };
        if session.submit_command(command).is_ok() { 1 } else { 0 }
    })).unwrap_or(0)
}

#[no_mangle]
pub extern "system" fn Java_pl_mkrstudio_truefootball3_lab_NativeBridge_nativeAdvanceV4Session<'local>(
    env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: i64,
    max_steps: i32,
) -> jstring {
    let response = catch_unwind(AssertUnwindSafe(|| {
        let entry = match v4_lookup_session(handle) {
            Ok(s) => s, Err(e) => return v4_error_json(&e),
        };
        let mut session = match entry.lock() {
            Ok(s) => s, Err(_) => return v4_error_json("V4 session poisoned"),
        };
        match session.advance_steps(max_steps.clamp(1, 500) as usize) {
            Ok(step) => serde_json::to_string(&step)
                .unwrap_or_else(|e| v4_error_json(&format!("V4 step serialize error: {e}"))),
            Err(e) => v4_error_json(&e.to_string()),
        }
    }))
    .unwrap_or_else(|_| v4_error_json("Rust panic inside nativeAdvanceV4Session"));
    env.new_string(&response).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "system" fn Java_pl_mkrstudio_truefootball3_lab_NativeBridge_nativeFinishV4Session<'local>(
    env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: i64,
) -> jstring {
    let response = catch_unwind(AssertUnwindSafe(|| {
        let entry = match v4_lookup_session(handle) {
            Ok(s) => s, Err(e) => return v4_error_json(&e),
        };
        let mut session = match entry.lock() {
            Ok(s) => s, Err(_) => return v4_error_json("V4 session poisoned"),
        };
        match session.run_to_full_time() {
            Ok(result) => serde_json::to_string(&result)
                .unwrap_or_else(|e| v4_error_json(&format!("V4 finish serialize error: {e}"))),
            Err(e) => v4_error_json(&e.to_string()),
        }
    }))
    .unwrap_or_else(|_| v4_error_json("Rust panic inside nativeFinishV4Session"));
    env.new_string(&response).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "system" fn Java_pl_mkrstudio_truefootball3_lab_NativeBridge_nativeDestroyV4Session<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: i64,
) {
    if handle <= 0 { return; }
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if let Ok(mut registry) = v4_registry().lock() {
            // Removing the registry entry stops NEW requests. Running requests
            // own an Arc and finish before the Rust memory is released.
            registry.remove(&handle);
        }
    }));
}

/// Safely replay an interactive session including every accepted tactical or
/// substitution command at the exact simulation-beat boundary.
#[no_mangle]
pub extern "system" fn Java_pl_mkrstudio_truefootball3_lab_NativeBridge_nativeReplayV4Json<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    input_json: SafeJString<'local>,
    command_timeline_json: SafeJString<'local>,
) -> jstring {
    let response = catch_unwind(AssertUnwindSafe(|| {
        let raw: String = match env.get_string(&input_json) {
            Ok(s) => s.to_string_lossy().into_owned(),
            Err(e) => return v4_error_json(&format!("JNI input read error: {e}")),
        };
        let history: String = match env.get_string(&command_timeline_json) {
            Ok(s) => s.to_string_lossy().into_owned(),
            Err(e) => return v4_error_json(&format!("JNI command read error: {e}")),
        };
        let input: v4::MatchInputV4 = match serde_json::from_str(&raw) {
            Ok(i) => i,
            Err(e) => return v4_error_json(&format!("V4 JSON input error: {e}")),
        };
        let commands: Vec<v4::AppliedCommandV4> = match serde_json::from_str(&history) {
            Ok(c) => c,
            Err(e) => return v4_error_json(&format!("V4 command timeline error: {e}")),
        };
        match v4::replay_v4_with_timeline(input, &commands) {
            Ok(result) => serde_json::to_string(&result)
                .unwrap_or_else(|e| v4_error_json(&format!("V4 replay serialize error: {e}"))),
            Err(e) => v4_error_json(&e.to_string()),
        }
    }))
    .unwrap_or_else(|_| v4_error_json("Rust panic inside nativeReplayV4Json"));
    env.new_string(&response).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut())
}
