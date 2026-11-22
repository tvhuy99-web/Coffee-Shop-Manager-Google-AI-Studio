package pl.mkrstudio.truefootball3.lab

import org.json.JSONObject
import java.io.Closeable

class LabRustLiveSession private constructor(private var handle: Long) : Closeable {
    companion object {
        fun open(inputJson: String): LabRustLiveSession? {
            if (!NativeBridge.isFullEngineReady()) return null
            val h = NativeBridge.nativeCreateSession(inputJson)
            LabLogStore.log("JNI", "nativeCreateSession -> handle=$h (inputBytes=${inputJson.length})")
            return if (h != 0L) LabRustLiveSession(h) else null
        }
    }

    val isOpen: Boolean get() = handle != 0L

    fun submitTeamInstruction(gameSecond: Int, side: String, commandType: String, value: Int): Boolean {
        if (handle == 0L) return false
        val payload = JSONObject()
            .put("gameSecond", gameSecond.coerceAtLeast(0))
            .put("side", side)
            .put("type", commandType)
            .put("intValue", value.coerceIn(0, 100))
            .toString()
        val ok = NativeBridge.nativeSubmitCommand(handle, payload) == 1
        LabLogStore.log("CMD", "submitCommand ok=$ok payload=$payload")
        return ok
    }

    fun submitSubstitution(gameSecond: Int, side: String, playerOutId: String, playerInId: String): Boolean {
        if (handle == 0L) return false
        val payload = JSONObject()
            .put("gameSecond", gameSecond.coerceAtLeast(0))
            .put("side", side)
            .put("type", "SUBSTITUTE")
            .put("playerId", playerOutId)
            .put("secondaryPlayerId", playerInId)
            .toString()
        val ok = NativeBridge.nativeSubmitCommand(handle, payload) == 1
        LabLogStore.log("CMD", "submitCommand ok=$ok payload=$payload")
        return ok
    }

    fun advance(maxSteps: Int = 12): JSONObject {
        if (handle == 0L) return JSONObject().put("ok", false).put("error", "session closed")
        return JSONObject(NativeBridge.nativeAdvanceSession(handle, maxSteps))
    }

    fun finish(): JSONObject {
        if (handle == 0L) return JSONObject().put("ok", false).put("error", "session closed")
        return JSONObject(NativeBridge.nativeFinishSession(handle))
    }

    override fun close() {
        val h = handle
        handle = 0L
        if (h != 0L) {
            LabLogStore.log("JNI", "nativeDestroySession handle=$h")
            NativeBridge.nativeDestroySession(h)
        }
    }
}
