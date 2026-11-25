package pl.mkrstudio.truefootball3.lab

import org.json.JSONArray
import org.json.JSONObject
import java.io.Closeable

/**
 * Live interactive Rust match session wrapper for Match Engine 4.0 / 4.1 DES.
 */
class LabRustLiveSession private constructor(
    @Volatile private var handle: Long,
    val engineVersion: LabEngineVersion,
    private var v4HomeInstructions: JSONObject
) : Closeable {
    companion object {
        fun open(
            inputJson: String,
            engineVersion: LabEngineVersion = LabEngineVersion.V4
        ): LabRustLiveSession? {
            if (!NativeBridge.isV4InteractiveReady()) return null
            val h = NativeBridge.createV4Session(inputJson)
            LabLogStore.log("JNI", "nativeCreateV4Session(V4) -> handle=$h (inputBytes=${inputJson.length})")
            val initialInstructions = extractV4HomeInstructions(inputJson)
            return if (h != 0L) LabRustLiveSession(h, engineVersion, initialInstructions) else null
        }

        /**
         * Restores a live V4.1 DES session from a verified Rust checkpoint JSON snapshot
         * via `NativeBridge.nativeImportV4Checkpoint`.
         */
        fun importFromCheckpoint(
            checkpointJson: String,
            initialHomeInstructions: JSONObject? = null
        ): LabRustLiveSession? {
            if (!NativeBridge.isV4DesReady() || checkpointJson.isBlank()) return null
            val h = NativeBridge.importV4Checkpoint(checkpointJson)
            LabLogStore.log(
                "JNI",
                "nativeImportV4Checkpoint(V4) -> handle=$h (checkpointBytes=${checkpointJson.length})"
            )
            if (h <= 0L) return null
            val instructions = initialHomeInstructions?.let { JSONObject(it.toString()) }
                ?: extractV4HomeInstructionsFromCheckpoint(checkpointJson)
            return LabRustLiveSession(h, LabEngineVersion.V4, instructions)
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

        private fun extractV4HomeInstructionsFromCheckpoint(checkpointJson: String): JSONObject {
            return try {
                val payload = JSONObject(checkpointJson).optJSONObject("payload")
                val stateObj = payload?.optJSONObject("state")
                val stateInstructions = stateObj?.optJSONObject("activeHomeTi")
                    ?: stateObj?.optJSONObject("home")?.optJSONObject("instructions")
                val inputInstructions = payload
                    ?.optJSONObject("input")
                    ?.optJSONObject("homeTeam")
                    ?.optJSONObject("teamInstructions")
                val resolved = stateInstructions ?: inputInstructions
                if (resolved != null) JSONObject(resolved.toString()) else defaultV4Instructions()
            } catch (_: Exception) {
                defaultV4Instructions()
            }
        }
    }

    val isOpen: Boolean get() = handle != 0L

    /** Builds a schema-valid Rust MatchCommandV4::UpdateTactics JSON payload from current instructions. */
    @Synchronized
    fun buildV4UpdateTacticsCommandJson(side: String, commandType: String, value: Int): String {
        val clamped = value.coerceIn(0, 100)
        val instructions = JSONObject(v4HomeInstructions.toString())
        when (commandType) {
            "CHANGE_PRESSING" -> instructions.put("pressingIntensity", clamped)
            "CHANGE_TEMPO" -> instructions.put("tempo", clamped)
            "CHANGE_LINE_HEIGHT" -> instructions.put("lineHeight", clamped)
            "CHANGE_MENTALITY" -> instructions.put("mentality", clamped)
            "CHANGE_WIDTH" -> instructions.put("width", clamped)
            "CHANGE_TIME_WASTING" -> instructions.put("timeWasting", clamped)
            else -> instructions.put("tempo", clamped)
        }
        return JSONObject()
            .put("type", "UPDATE_TACTICS")
            .put("side", side)
            .put("instructions", instructions)
            .toString()
    }

    @Synchronized
    fun submitTeamInstruction(gameSecond: Int, side: String, commandType: String, value: Int): Boolean {
        if (handle == 0L) return false
        val clamped = value.coerceIn(0, 100)
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
        return ok
    }

    @Synchronized
    fun submitSubstitution(gameSecond: Int, side: String, playerOutId: String, playerInId: String): Boolean {
        if (handle == 0L) return false
        val payload = JSONObject()
            .put("type", "SUBSTITUTE")
            .put("side", side)
            .put("playerOutId", playerOutId)
            .put("playerInId", playerInId)
            .toString()
        val ok = NativeBridge.submitV4Command(handle, payload)
        LabLogStore.log("CMD", "submitSub(V4) ok=$ok payload=$payload")
        return ok
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
        return JSONObject(NativeBridge.advanceV4Session(handle, maxSteps))
    }

    /**
     * Advances the V4.1 DES session up to [targetSimTimeMs] (bounded by [maxStepsGuard])
     * and returns the raw `SessionStepResultV4` JSON response from Rust.
     */
    @Synchronized
    fun advanceUntilMs(targetSimTimeMs: Long, maxStepsGuard: Int = 500): String {
        if (handle == 0L) return """{"ok":false,"error":"session closed"}"""
        return NativeBridge.advanceV4SessionUntilMs(handle, targetSimTimeMs, maxStepsGuard)
    }

    /**
     * Explicitly transitions the V4.1 DES session from half-time / extra-time break into the next period.
     */
    @Synchronized
    fun startNextPeriod(): String {
        if (handle == 0L) return """{"ok":false,"error":"session closed"}"""
        val raw = NativeBridge.startNextPeriod(handle)
        LabLogStore.log("JNI", "nativeStartNextPeriod(V4) handle=$handle bytes=${raw.length}")
        return raw
    }

    /**
     * Exports a self-verifying Rust `MatchCheckpointV4` JSON snapshot of the active V4.1 DES session.
     */
    @Synchronized
    fun exportCheckpointJson(): String {
        if (handle == 0L) return """{"ok":false,"error":"session closed"}"""
        return NativeBridge.exportV4Checkpoint(handle)
    }

    /**
     * Submits a raw V4.1 DES command JSON payload and returns the structured JSON response from Rust.
     */
    @Synchronized
    fun submitCommandJson(commandJson: String): String {
        if (handle == 0L) return """{"ok":false,"error":"session closed"}"""
        val raw = NativeBridge.submitV4CommandJson(handle, commandJson)
        try {
            val res = JSONObject(raw)
            if (res.optBoolean("ok", false)) {
                val req = JSONObject(commandJson)
                if (req.optString("type") == "UPDATE_TACTICS" && req.optString("side") == "HOME") {
                    req.optJSONObject("instructions")?.let {
                        v4HomeInstructions = JSONObject(it.toString())
                    }
                }
            }
        } catch (_: Exception) {
            // Keep raw Rust response untouched even if command JSON is non-tactical or malformed.
        }
        LabLogStore.log("CMD", "submitCommandJson(V4) bytes=${commandJson.length}")
        return raw
    }

    @Synchronized
    fun finish(): JSONObject {
        if (handle == 0L) return JSONObject().put("ok", false).put("error", "session closed")
        return JSONObject(NativeBridge.finishV4Session(handle))
    }

    @Synchronized
    override fun close() {
        val h = handle
        handle = 0L
        if (h != 0L) {
            LabLogStore.log("JNI", "nativeDestroyV4Session(V4) handle=$h")
            NativeBridge.destroyV4Session(h)
        }
    }
}
