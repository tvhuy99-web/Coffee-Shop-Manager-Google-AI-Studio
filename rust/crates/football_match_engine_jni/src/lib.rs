use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use jni::objects::{JClass, JString as SafeJString};
use jni::sys::jstring;
use jni::JNIEnv;

use std::ffi::c_void;

use match_engine::contracts::VersionManifest4;
use match_engine::v4;

// ============================================================================
// Match Engine V4.1 DES Safe JNI Entrypoints (jni crate + serde_json + rayon)
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
// Match Engine V4.1 DES Interactive JNI. Handles are opaque, monotonic IDs,
// NEVER memory addresses. Removal from the registry is safe during concurrent
// advances because callers hold Arc references and a per-session mutex.
// ============================================================================

type V4SessionArc = Arc<Mutex<v4::MatchSessionV4>>;
static V4_SESSIONS: OnceLock<Mutex<HashMap<i64, V4SessionArc>>> = OnceLock::new();
static NEXT_V4_SESSION_ID: AtomicI64 = AtomicI64::new(1);

fn v4_registry() -> &'static Mutex<HashMap<i64, V4SessionArc>> {
    V4_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn v4_register_session(session: v4::MatchSessionV4) -> i64 {
    let id = NEXT_V4_SESSION_ID.fetch_add(1, Ordering::Relaxed);
    if id <= 0 {
        return 0;
    }
    match v4_registry().lock() {
        Ok(mut registry) => {
            registry.insert(id, Arc::new(Mutex::new(session)));
            id
        }
        Err(_) => 0,
    }
}

fn v4_lookup_session(handle: i64) -> Result<V4SessionArc, String> {
    if handle <= 0 {
        return Err("invalid V4 session ID".into());
    }
    let map = v4_registry().lock().map_err(|_| "V4 session registry poisoned")?;
    map.get(&handle)
        .cloned()
        .ok_or_else(|| "V4 session already closed or unknown".into())
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
        v4_register_session(session)
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
        let entry = match v4_lookup_session(handle) {
            Ok(e) => e,
            Err(_) => return 0,
        };
        let mut session = match entry.lock() {
            Ok(s) => s,
            Err(_) => return 0,
        };
        if session.submit_command(command).is_ok() { 1 } else { 0 }
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub extern "system" fn Java_pl_mkrstudio_truefootball3_lab_NativeBridge_nativeSubmitV4CommandJson<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: i64,
    command_json: SafeJString<'local>,
) -> jstring {
    let response = catch_unwind(AssertUnwindSafe(|| {
        let raw: String = match env.get_string(&command_json) {
            Ok(s) => s.to_string_lossy().into_owned(),
            Err(e) => return v4_error_json(&format!("JNI command read error: {e}")),
        };
        let command: v4::MatchCommandV4 = match serde_json::from_str(&raw) {
            Ok(c) => c,
            Err(e) => return v4_error_json(&format!("V4 command JSON parse error: {e}")),
        };
        let entry = match v4_lookup_session(handle) {
            Ok(e) => e,
            Err(e) => return v4_error_json(&e),
        };
        let mut session = match entry.lock() {
            Ok(s) => s,
            Err(_) => return v4_error_json("V4 session poisoned"),
        };
        match session.submit_command(command) {
            Ok(()) => serde_json::json!({
                "ok": true,
                "protocolVersion": v4::ENGINE_PROTOCOL_VERSION_V4,
                "engineVersion": v4::ENGINE_VERSION_V4,
                "simTimeMs": session.sim_time_ms(),
            })
            .to_string(),
            Err(e) => v4_error_json(&e.to_string()),
        }
    }))
    .unwrap_or_else(|_| v4_error_json("Rust panic inside nativeSubmitV4CommandJson"));

    env.new_string(&response)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
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
            Ok(s) => s,
            Err(e) => return v4_error_json(&e),
        };
        let mut session = match entry.lock() {
            Ok(s) => s,
            Err(_) => return v4_error_json("V4 session poisoned"),
        };
        match session.advance_steps(max_steps.clamp(1, 500) as usize) {
            Ok(step) => serde_json::to_string(&step)
                .unwrap_or_else(|e| v4_error_json(&format!("V4 step serialize error: {e}"))),
            Err(e) => v4_error_json(&e.to_string()),
        }
    }))
    .unwrap_or_else(|_| v4_error_json("Rust panic inside nativeAdvanceV4Session"));
    env.new_string(&response)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "system" fn Java_pl_mkrstudio_truefootball3_lab_NativeBridge_nativeAdvanceV4SessionUntilMs<'local>(
    env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: i64,
    target_sim_time_ms: i64,
    max_steps_guard: i32,
) -> jstring {
    let response = catch_unwind(AssertUnwindSafe(|| {
        let entry = match v4_lookup_session(handle) {
            Ok(s) => s,
            Err(e) => return v4_error_json(&e),
        };
        let mut session = match entry.lock() {
            Ok(s) => s,
            Err(_) => return v4_error_json("V4 session poisoned"),
        };
        let guard = max_steps_guard.clamp(1, 10_000) as usize;
        match session.advance_until_ms(target_sim_time_ms, guard) {
            Ok(step) => serde_json::to_string(&step)
                .unwrap_or_else(|e| v4_error_json(&format!("V4 DES step serialize error: {e}"))),
            Err(e) => v4_error_json(&e.to_string()),
        }
    }))
    .unwrap_or_else(|_| v4_error_json("Rust panic inside nativeAdvanceV4SessionUntilMs"));
    env.new_string(&response)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "system" fn Java_pl_mkrstudio_truefootball3_lab_NativeBridge_nativeStartNextPeriod<'local>(
    env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: i64,
) -> jstring {
    let response = catch_unwind(AssertUnwindSafe(|| {
        let entry = match v4_lookup_session(handle) {
            Ok(s) => s,
            Err(e) => return v4_error_json(&e),
        };
        let mut session = match entry.lock() {
            Ok(s) => s,
            Err(_) => return v4_error_json("V4 session poisoned"),
        };
        match session.start_next_period() {
            Ok(step) => serde_json::to_string(&step)
                .unwrap_or_else(|e| v4_error_json(&format!("V4 startNextPeriod serialize error: {e}"))),
            Err(e) => v4_error_json(&e.to_string()),
        }
    }))
    .unwrap_or_else(|_| v4_error_json("Rust panic inside nativeStartNextPeriod"));
    env.new_string(&response)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "system" fn Java_pl_mkrstudio_truefootball3_lab_NativeBridge_nativeExportV4Checkpoint<'local>(
    env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: i64,
) -> jstring {
    let response = catch_unwind(AssertUnwindSafe(|| {
        let entry = match v4_lookup_session(handle) {
            Ok(s) => s,
            Err(e) => return v4_error_json(&e),
        };
        let session = match entry.lock() {
            Ok(s) => s,
            Err(_) => return v4_error_json("V4 session poisoned"),
        };
        match session.export_checkpoint() {
            Ok(ckpt) => serde_json::to_string(&ckpt)
                .unwrap_or_else(|e| v4_error_json(&format!("V4 checkpoint serialize error: {e}"))),
            Err(e) => v4_error_json(&e.to_string()),
        }
    }))
    .unwrap_or_else(|_| v4_error_json("Rust panic inside nativeExportV4Checkpoint"));
    env.new_string(&response)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "system" fn Java_pl_mkrstudio_truefootball3_lab_NativeBridge_nativeImportV4Checkpoint<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    checkpoint_json: SafeJString<'local>,
) -> i64 {
    catch_unwind(AssertUnwindSafe(|| {
        let raw: String = match env.get_string(&checkpoint_json) {
            Ok(s) => s.to_string_lossy().into_owned(),
            Err(_) => return 0,
        };
        let ckpt: v4::MatchCheckpointV4 = match serde_json::from_str(&raw) {
            Ok(c) => c,
            Err(_) => return 0,
        };
        let session = match v4::MatchSessionV4::from_checkpoint(ckpt) {
            Ok(s) => s,
            Err(_) => return 0,
        };
        v4_register_session(session)
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub extern "system" fn Java_pl_mkrstudio_truefootball3_lab_NativeBridge_nativeFinishV4Session<'local>(
    env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: i64,
) -> jstring {
    let response = catch_unwind(AssertUnwindSafe(|| {
        let entry = match v4_lookup_session(handle) {
            Ok(s) => s,
            Err(e) => return v4_error_json(&e),
        };
        let mut session = match entry.lock() {
            Ok(s) => s,
            Err(_) => return v4_error_json("V4 session poisoned"),
        };
        match session.run_to_full_time() {
            Ok(result) => serde_json::to_string(&result)
                .unwrap_or_else(|e| v4_error_json(&format!("V4 finish serialize error: {e}"))),
            Err(e) => v4_error_json(&e.to_string()),
        }
    }))
    .unwrap_or_else(|_| v4_error_json("Rust panic inside nativeFinishV4Session"));
    env.new_string(&response)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "system" fn Java_pl_mkrstudio_truefootball3_lab_NativeBridge_nativeDestroyV4Session<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: i64,
) {
    if handle <= 0 {
        return;
    }
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if let Ok(mut registry) = v4_registry().lock() {
            registry.remove(&handle);
        }
    }));
}

/// Safely replay an interactive session including every accepted tactical or
/// substitution command at the exact DES step & millisecond boundary.
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
    env.new_string(&response)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

/// Returns the serialized `VersionManifest4` describing build, schema, and gameplay identity.
#[no_mangle]
pub extern "system" fn Java_pl_mkrstudio_truefootball3_lab_NativeBridge_nativeGetVersionManifestJson<'local>(
    env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> jstring {
    let response = catch_unwind(AssertUnwindSafe(|| {
        let manifest = VersionManifest4::current_engine_manifest();
        serde_json::to_string(&manifest)
            .unwrap_or_else(|e| v4_error_json(&format!("VersionManifest4 serialize error: {e}")))
    }))
    .unwrap_or_else(|_| v4_error_json("Rust panic inside nativeGetVersionManifestJson"));
    env.new_string(&response)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

/// Separate JNI bridge version for V4.1 DES capabilities.
#[no_mangle]
pub extern "system" fn Java_pl_mkrstudio_truefootball3_lab_NativeBridge_nativeBridgeVersion(
    _env: *mut c_void,
    _class: *mut c_void,
) -> i32 {
    v4::JNI_BRIDGE_VERSION
}

/// Highest supported Match Engine wire protocol version (`5` = V4.1 DES 54-attribute 90-minute protocol).
#[no_mangle]
pub extern "system" fn Java_pl_mkrstudio_truefootball3_lab_NativeBridge_nativeEngineProtocolVersion(
    _env: *mut c_void,
    _class: *mut c_void,
) -> i32 {
    v4::ENGINE_PROTOCOL_VERSION_V4 as i32
}
