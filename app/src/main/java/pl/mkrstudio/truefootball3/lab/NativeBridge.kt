package pl.mkrstudio.truefootball3.lab

object NativeBridge {
    val loaded: Boolean = try {
        System.loadLibrary("football_match_engine")
        true
    } catch (_: UnsatisfiedLinkError) {
        false
    }

    // --- V3 Reference Engine JNI (preserved 100% for parity/fallback) ---
    external fun nativePortVersion(): Int
    external fun rngSample(seed: Long, bound: Int): Int
    external fun nativeCoreSelfCheck(): Int
    external fun nativeSimulateMatchJson(inputJson: String): String
    external fun nativeCreateSession(inputJson: String): Long
    external fun nativeSubmitCommand(handle: Long, commandJson: String): Int
    external fun nativeAdvanceSession(handle: Long, maxSteps: Int): String
    external fun nativeFinishSession(handle: Long): String
    external fun nativeDestroySession(handle: Long)

    // --- V4.0 Engine JNI (batch, single, and interactive step-by-step session) ---
    external fun nativeBridgeVersion(): Int
    external fun nativeEngineProtocolVersion(): Int
    external fun nativeSimulateMatchV4Json(inputJson: String): String
    external fun nativeSimulateBatchV4Json(inputsJson: String): String
    external fun nativeCreateV4Session(inputJson: String): Long
    external fun nativeSubmitV4Command(handle: Long, commandJson: String): Int
    external fun nativeAdvanceV4Session(handle: Long, maxSteps: Int): String
    external fun nativeFinishV4Session(handle: Long): String
    external fun nativeDestroyV4Session(handle: Long)
    external fun nativeReplayV4Json(inputJson: String, commandTimelineJson: String): String

    fun isFullEngineReady(): Boolean = loaded && try {
        nativePortVersion() >= 4
    } catch (_: UnsatisfiedLinkError) {
        false
    }

    fun isV4EngineReady(): Boolean = loaded && try {
        nativeBridgeVersion() >= 5 && nativeEngineProtocolVersion() >= 4
    } catch (_: UnsatisfiedLinkError) {
        false
    }

    fun isV4InteractiveReady(): Boolean = loaded && try {
        nativeBridgeVersion() >= 7 && nativeEngineProtocolVersion() >= 4
    } catch (_: UnsatisfiedLinkError) {
        false
    }

    fun simulateMatch(inputJson: String): String {
        if (!loaded) return """{"ok":false,"error":"Rust native library not packaged"}"""
        return try {
            nativeSimulateMatchJson(inputJson)
        } catch (e: UnsatisfiedLinkError) {
            """{"ok":false,"error":"JNI symbol mismatch: ${e.message}"}"""
        }
    }

    fun simulateMatchV4(inputJson: String): String {
        if (!loaded) return """{"ok":false,"error":"Rust native library not packaged"}"""
        return try {
            nativeSimulateMatchV4Json(inputJson)
        } catch (e: UnsatisfiedLinkError) {
            """{"ok":false,"error":"V4 JNI symbol mismatch: ${e.message}"}"""
        }
    }

    fun simulateBatchV4(inputsJson: String): String {
        if (!loaded) return """{"ok":false,"error":"Rust native library not packaged"}"""
        return try {
            nativeSimulateBatchV4Json(inputsJson)
        } catch (e: UnsatisfiedLinkError) {
            """{"ok":false,"error":"V4 batch JNI symbol mismatch: ${e.message}"}"""
        }
    }

    fun createV4Session(inputJson: String): Long =
        if (!loaded) 0L else try { nativeCreateV4Session(inputJson) } catch (_: UnsatisfiedLinkError) { 0L }

    fun submitV4Command(handle: Long, commandJson: String): Boolean =
        if (!loaded || handle == 0L) false else try { nativeSubmitV4Command(handle, commandJson) == 1 } catch (_: UnsatisfiedLinkError) { false }

    fun advanceV4Session(handle: Long, maxSteps: Int = 1): String =
        if (!loaded || handle == 0L) """{"ok":false,"error":"Invalid V4 session"}"""
        else try { nativeAdvanceV4Session(handle, maxSteps) } catch (e: UnsatisfiedLinkError) { """{"ok":false,"error":"${e.message}"}""" }

    fun finishV4Session(handle: Long): String =
        if (!loaded || handle == 0L) """{"ok":false,"error":"Invalid V4 session"}"""
        else try { nativeFinishV4Session(handle) } catch (e: UnsatisfiedLinkError) { """{"ok":false,"error":"${e.message}"}""" }

    fun replayMatchV4(inputJson: String, commandTimelineJson: String): String {
        if (!isV4InteractiveReady()) return """{"ok":false,"error":"V4 replay unavailable: requires JNI bridge 7"}"""
        return try {
            nativeReplayV4Json(inputJson, commandTimelineJson)
        } catch (e: UnsatisfiedLinkError) {
            """{"ok":false,"error":"V4 replay JNI symbol missing"}"""
        }
    }

    fun destroyV4Session(handle: Long) {
        if (loaded && handle != 0L) {
            try { nativeDestroyV4Session(handle) } catch (_: UnsatisfiedLinkError) {}
        }
    }

    fun status(): String = if (!loaded) {
        "Rust native chưa được đóng gói (.so)"
    } else {
        try {
            val ver = nativePortVersion()
            val check = Integer.bitCount(nativeCoreSelfCheck())
            if (isV4InteractiveReady()) {
                val bridgeVer = nativeBridgeVersion()
                val protoVer = nativeEngineProtocolVersion()
                "Rust V4 tương tác sẵn sàng (bridge v$bridgeVer • protocol v$protoVer • V3 self-check $check/5)"
            } else if (isV4EngineReady()) {
                "Rust V4 batch sẵn sàng (cần build lại .so v6 cho phiên tương tác)"
            } else if (ver >= 4) {
                "Rust engine v$ver sẵn sàng (self-check $check/5)"
            } else {
                "Rust port v$ver: self-check $check/5 (cần build lại .so mới)"
            }
        } catch (_: UnsatisfiedLinkError) {
            "Thư viện Rust không khớp bản Kotlin"
        }
    }
}
