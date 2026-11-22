use std::ffi::{c_char, c_void, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::{
    discipline,
    geometry::TeamSide,
    session::{MatchSession, UnifiedMatchEngine},
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
