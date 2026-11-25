package pl.mkrstudio.truefootball3.lab

object NativeBridge {
    val loaded: Boolean = try {
        System.loadLibrary("football_match_engine")
        true
    } catch (_: UnsatisfiedLinkError) {
        false
    }

    // --- V4.0 / V4.1 DES Engine JNI ---
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
    external fun nativeAdvanceV4SessionUntilMs(handle: Long, targetSimTimeMs: Long, maxStepsGuard: Int): String
    external fun nativeStartNextPeriod(handle: Long): String
    external fun nativeExportV4Checkpoint(handle: Long): String
    external fun nativeImportV4Checkpoint(checkpointJson: String): Long
    external fun nativeSubmitV4CommandJson(handle: Long, commandJson: String): String
    external fun nativeGetVersionManifestJson(): String

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

    fun isV4DesReady(): Boolean = loaded && try {
        nativeBridgeVersion() >= 8 && nativeEngineProtocolVersion() >= 5
    } catch (_: UnsatisfiedLinkError) {
        false
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

    fun submitV4CommandJson(handle: Long, commandJson: String): String =
        if (!loaded || handle == 0L) """{"ok":false,"error":"Invalid V4 session"}"""
        else try { nativeSubmitV4CommandJson(handle, commandJson) } catch (e: UnsatisfiedLinkError) { """{"ok":false,"error":"V4 command JSON JNI symbol missing (requires bridge v8): ${e.message}"}""" }

    fun advanceV4Session(handle: Long, maxSteps: Int = 1): String =
        if (!loaded || handle == 0L) """{"ok":false,"error":"Invalid V4 session"}"""
        else try { nativeAdvanceV4Session(handle, maxSteps) } catch (e: UnsatisfiedLinkError) { """{"ok":false,"error":"${e.message}"}""" }

    fun advanceV4SessionUntilMs(handle: Long, targetSimTimeMs: Long, maxStepsGuard: Int = 500): String =
        if (!loaded || handle == 0L) """{"ok":false,"error":"Invalid V4 session"}"""
        else try { nativeAdvanceV4SessionUntilMs(handle, targetSimTimeMs, maxStepsGuard) } catch (e: UnsatisfiedLinkError) { """{"ok":false,"error":"V4 DES advance JNI symbol missing (requires bridge v8): ${e.message}"}""" }

    fun startNextPeriod(handle: Long): String =
        if (!loaded || handle == 0L) """{"ok":false,"error":"Invalid V4 session"}"""
        else try { nativeStartNextPeriod(handle) } catch (e: UnsatisfiedLinkError) { """{"ok":false,"error":"V4 DES startNextPeriod JNI symbol missing (requires bridge v8): ${e.message}"}""" }

    fun exportV4Checkpoint(handle: Long): String =
        if (!loaded || handle == 0L) """{"ok":false,"error":"Invalid V4 session"}"""
        else try { nativeExportV4Checkpoint(handle) } catch (e: UnsatisfiedLinkError) { """{"ok":false,"error":"V4 DES exportCheckpoint JNI symbol missing (requires bridge v8): ${e.message}"}""" }

    fun importV4Checkpoint(checkpointJson: String): Long =
        if (!loaded || checkpointJson.isBlank()) 0L
        else try { nativeImportV4Checkpoint(checkpointJson) } catch (_: UnsatisfiedLinkError) { 0L }

    fun finishV4Session(handle: Long): String =
        if (!loaded || handle == 0L) """{"ok":false,"error":"Invalid V4 session"}"""
        else try { nativeFinishV4Session(handle) } catch (e: UnsatisfiedLinkError) { """{"ok":false,"error":"${e.message}"}""" }

    fun replayMatchV4(inputJson: String, commandTimelineJson: String): String {
        if (!isV4InteractiveReady()) return """{"ok":false,"error":"V4 replay unavailable: requires JNI bridge v8"}"""
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

    fun getVersionManifestJson(): String {
        if (!loaded) return """{"ok":false,"error":"Rust native library not packaged"}"""
        return try {
            nativeGetVersionManifestJson()
        } catch (e: UnsatisfiedLinkError) {
            """{"ok":false,"error":"VersionManifest4 JNI symbol missing: ${e.message}"}"""
        }
    }

    fun status(): String = if (!loaded) {
        "Rust native chưa được đóng gói (.so)"
    } else {
        try {
            val bridgeVer = nativeBridgeVersion()
            val protoVer = nativeEngineProtocolVersion()
            when {
                isV4DesReady() -> "Rust V4.1 DES sẵn sàng (bridge v$bridgeVer • protocol v$protoVer)"
                isV4InteractiveReady() -> "Rust V4 tương tác sẵn sàng (bridge v$bridgeVer • protocol v$protoVer • cần bridge v8 cho DES)"
                isV4EngineReady() -> "Rust V4 batch sẵn sàng (cần build lại .so bridge v8 cho phiên tương tác)"
                else -> "Thư viện Rust V4 chưa tương thích (bridge v$bridgeVer • protocol v$protoVer)"
            }
        } catch (_: UnsatisfiedLinkError) {
            "Thư viện Rust không khớp bản Kotlin"
        }
    }
}
