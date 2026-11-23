package pl.mkrstudio.truefootball3.lab

import org.json.JSONObject
import org.json.JSONArray
import java.io.Closeable

/**
 * Unified live interactive Rust match session wrapper supporting both
 * Match Engine 3.0 and Match Engine 4.0 behind the same Kotlin interface.
 */
class LabRustLiveSession private constructor(
    @Volatile private var handle: Long,
    val engineVersion: LabEngineVersion,
    private var v4HomeInstructions: JSONObject
) : Closeable {
    companion object {
        fun open(
            inputJson: String,
            engineVersion: LabEngineVersion = LabEngineVersion.V3
        ): LabRustLiveSession? {
            return when (engineVersion) {
                LabEngineVersion.V3 -> {
                    if (!NativeBridge.isFullEngineReady()) return null
                    val h = NativeBridge.nativeCreateSession(inputJson)
                    LabLogStore.log("JNI", "nativeCreateSession(V3) -> handle=$h (inputBytes=${inputJson.length})")
                    if (h != 0L) LabRustLiveSession(h, LabEngineVersion.V3, defaultV4Instructions()) else null
                }
                LabEngineVersion.V4 -> {
                    if (!NativeBridge.isV4InteractiveReady()) return null
                    val h = NativeBridge.createV4Session(inputJson)
                    LabLogStore.log("JNI", "nativeCreateV4Session(V4) -> handle=$h (inputBytes=${inputJson.length})")
                    val initialInstructions = extractV4HomeInstructions(inputJson)
                    if (h != 0L) LabRustLiveSession(h, LabEngineVersion.V4, initialInstructions) else null
                }
            }
        }

        private fun defaultV4Instructions(): JSONObject = JSONObject()
            .put("pressingIntensity", 50)
            .put("tempo", 50)
            .put("lineHeight", 50)
            .put("width", 50)
            .put("timeWasting", 10)
            .put("offsideTrap", false)
            .put("aggression", 50)
            .put("transitionRisk", 50)
            .put("buildUpRisk", 50)
            .put("concentration", 50)

        private fun extractV4HomeInstructions(inputJson: String): JSONObject {
            return try {
                val ti = JSONObject(inputJson)
                    .optJSONObject("homeTeam")
                    ?.optJSONObject("teamInstructions")
                if (ti != null) JSONObject(ti.toString()) else defaultV4Instructions()
            } catch (_: Exception) {
                defaultV4Instructions()
            }
        }
    }

    val isOpen: Boolean get() = handle != 0L

    @Synchronized
    fun submitTeamInstruction(gameSecond: Int, side: String, commandType: String, value: Int): Boolean {
        if (handle == 0L) return false
        val clamped = value.coerceIn(0, 100)
        return when (engineVersion) {
            LabEngineVersion.V3 -> {
                val payload = JSONObject()
                    .put("gameSecond", gameSecond.coerceAtLeast(0))
                    .put("side", side)
                    .put("type", commandType)
                    .put("intValue", clamped)
                    .toString()
                val ok = NativeBridge.nativeSubmitCommand(handle, payload) == 1
                LabLogStore.log("CMD", "submitCommand(V3) ok=$ok payload=$payload")
                ok
            }
            LabEngineVersion.V4 -> {
                val updated = JSONObject(v4HomeInstructions.toString())
                when (commandType) {
                    "CHANGE_PRESSING" -> updated.put("pressingIntensity", clamped)
                    "CHANGE_TEMPO" -> updated.put("tempo", clamped)
                    "CHANGE_LINE_HEIGHT" -> {
                        updated.put("lineHeight", clamped)
                        updated.put("offsideTrap", clamped >= 68)
                    }
                }
                val payload = JSONObject()
                    .put("type", "UPDATE_TACTICS")
                    .put("side", side)
                    .put("instructions", updated)
                    .toString()
                val ok = NativeBridge.submitV4Command(handle, payload)
                if (ok) v4HomeInstructions = updated
                LabLogStore.log("CMD", "submitCommand(V4) ok=$ok payload=$payload")
                ok
            }
        }
    }

    @Synchronized
    fun submitSubstitution(gameSecond: Int, side: String, playerOutId: String, playerInId: String): Boolean {
        if (handle == 0L) return false
        return when (engineVersion) {
            LabEngineVersion.V3 -> {
                val payload = JSONObject()
                    .put("gameSecond", gameSecond.coerceAtLeast(0))
                    .put("side", side)
                    .put("type", "SUBSTITUTE")
                    .put("playerId", playerOutId)
                    .put("secondaryPlayerId", playerInId)
                    .toString()
                val ok = NativeBridge.nativeSubmitCommand(handle, payload) == 1
                LabLogStore.log("CMD", "submitSub(V3) ok=$ok payload=$payload")
                ok
            }
            LabEngineVersion.V4 -> {
                val payload = JSONObject()
                    .put("type", "SUBSTITUTE")
                    .put("side", side)
                    .put("playerOutId", playerOutId)
                    .put("playerInId", playerInId)
                    .toString()
                val ok = NativeBridge.submitV4Command(handle, payload)
                LabLogStore.log("CMD", "submitSub(V4) ok=$ok payload=$payload")
                ok
            }
        }
    }

    /** All exchanges in this list belong to one actual substitution opportunity. */
    @Synchronized
    fun submitSubstitutions(
        gameSecond: Int,
        side: String,
        changes: List<Pair<String, String>>
    ): Boolean {
        if (handle == 0L || changes.isEmpty() || changes.size > 5) return false
        if (changes.map { it.first }.toSet().size != changes.size ||
            changes.map { it.second }.toSet().size != changes.size) return false
        if (engineVersion == LabEngineVersion.V3) {
            if (changes.size != 1) return false
            val (out, incoming) = changes.single()
            return submitSubstitution(gameSecond, side, out, incoming)
        }
        val items = JSONArray()
        for ((out, incoming) in changes) {
            items.put(JSONObject()
                .put("playerOutId", out)
                .put("playerInId", incoming))
        }
        val payload = JSONObject()
            .put("type", "SUBSTITUTE_BATCH")
            .put("side", side)
            .put("changes", items)
            .toString()
        val ok = NativeBridge.submitV4Command(handle, payload)
        LabLogStore.log("CMD", "submitSubstitutionBatch(V4) size=${changes.size} ok=$ok")
        return ok
    }

    @Synchronized
    fun advance(maxSteps: Int = 12): JSONObject {
        if (handle == 0L) return JSONObject().put("ok", false).put("error", "session closed")
        val raw = when (engineVersion) {
            LabEngineVersion.V3 -> NativeBridge.nativeAdvanceSession(handle, maxSteps)
            LabEngineVersion.V4 -> NativeBridge.advanceV4Session(handle, maxSteps)
        }
        return JSONObject(raw)
    }

    @Synchronized
    fun finish(): JSONObject {
        if (handle == 0L) return JSONObject().put("ok", false).put("error", "session closed")
        val raw = when (engineVersion) {
            LabEngineVersion.V3 -> NativeBridge.nativeFinishSession(handle)
            LabEngineVersion.V4 -> NativeBridge.finishV4Session(handle)
        }
        return JSONObject(raw)
    }

    @Synchronized
    override fun close() {
        val h = handle
        handle = 0L
        if (h != 0L) {
            when (engineVersion) {
                LabEngineVersion.V3 -> {
                    LabLogStore.log("JNI", "nativeDestroySession(V3) handle=$h")
                    NativeBridge.nativeDestroySession(h)
                }
                LabEngineVersion.V4 -> {
                    LabLogStore.log("JNI", "nativeDestroyV4Session(V4) handle=$h")
                    NativeBridge.destroyV4Session(h)
                }
            }
        }
    }
}
