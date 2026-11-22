package pl.mkrstudio.truefootball3.lab

object NativeBridge {
    val loaded: Boolean = try {
        System.loadLibrary("football_match_engine")
        true
    } catch (_: UnsatisfiedLinkError) {
        false
    }

    external fun nativePortVersion(): Int
    external fun rngSample(seed: Long, bound: Int): Int
    external fun nativeCoreSelfCheck(): Int
    external fun nativeSimulateMatchJson(inputJson: String): String
    external fun nativeCreateSession(inputJson: String): Long
    external fun nativeSubmitCommand(handle: Long, commandJson: String): Int
    external fun nativeAdvanceSession(handle: Long, maxSteps: Int): String
    external fun nativeFinishSession(handle: Long): String
    external fun nativeDestroySession(handle: Long)

    fun isFullEngineReady(): Boolean = loaded && try {
        nativePortVersion() >= 4
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

    fun status(): String = if (!loaded) {
        "Rust native chưa được đóng gói (.so)"
    } else {
        try {
            val ver = nativePortVersion()
            val check = Integer.bitCount(nativeCoreSelfCheck())
            if (ver >= 4) {
                "Rust engine v$ver sẵn sàng (self-check $check/5)"
            } else {
                "Rust port v$ver: self-check $check/5 (cần build lại .so mới)"
            }
        } catch (_: UnsatisfiedLinkError) {
            "Thư viện Rust không khớp bản Kotlin"
        }
    }
}
