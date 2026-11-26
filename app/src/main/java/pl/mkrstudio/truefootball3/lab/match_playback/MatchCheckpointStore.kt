package pl.mkrstudio.truefootball3.lab

import android.content.Context
import android.util.AtomicFile
import org.json.JSONObject
import java.io.File
import java.io.IOException
import java.nio.ByteBuffer
import java.security.MessageDigest
import java.util.zip.CRC32

/**
 * Mutable, crash-safe atomic checkpoint store for active in-progress V4.1 DES matches.
 *
 * Storage layout:
 *   `File(context.applicationContext.filesDir, "v4_active_checkpoints/active-<sha256(matchId)>.ckpt.json")`
 *
 * Guarantees:
 * 1. Atomic write via [AtomicFile] with explicit `flush()` + `fd.sync()`.
 * 2. Two-layer integrity verification:
 *    - `integritySha256`: length-prefixed SHA-256 over schema/engine/ruleset identity, match identity,
 *      committed `rustCheckpointJson`, and serialized `androidPlayback`.
 *    - `payloadCrc32`: [CRC32] over the canonical UTF-8 payload JSON (`rustCheckpointJson` + `androidPlayback`).
 * 3. Never persists transient WAV cache file paths or `SystemClock.elapsedRealtime()`.
 * 4. Checkpoint files are deleted once a match reaches `FULL_TIME`.
 */
class MatchCheckpointStore(context: Context) {
    companion object {
        const val CHECKPOINT_SCHEMA_VERSION = 2
        const val DEFAULT_ENGINE_VERSION = "v4.1.1-des"
        const val DEFAULT_PROTOCOL_VERSION = 5
        const val DEFAULT_EVENT_SCHEMA_VERSION = 5
        const val DEFAULT_RULESET_ID = "ifab_league_standard"
        const val DEFAULT_RULESET_VERSION = 1
        const val DEFAULT_TIMING_MODEL_VERSION = 1

        private const val CHECKPOINT_DIR_NAME = "v4_active_checkpoints"
        private const val FILE_PREFIX = "active-"
        private const val FILE_SUFFIX = ".ckpt.json"

        private val fileLock = Any()

        private fun sha256Hex(data: ByteArray): String =
            MessageDigest.getInstance("SHA-256").digest(data)
                .joinToString("") { "%02x".format(it) }

        private fun crc32Value(data: ByteArray): Long {
            val crc = CRC32()
            crc.update(data)
            return crc.value
        }

        private fun JSONObject.optNullableString(key: String): String? =
            if (!has(key) || isNull(key)) null else optString(key)

        private fun JSONObject.optNullableLong(key: String): Long? =
            if (!has(key) || isNull(key)) null else optLong(key)

        private fun serializePlaybackState(state: PlaybackCheckpointState): JSONObject =
            JSONObject()
                .put("playbackSimTimeMs", state.playbackSimTimeMs)
                .put("committedRustSimTimeMs", state.committedRustSimTimeMs)
                .put("playbackRate", state.playbackRate.toDouble())
                .put("paused", state.paused)
                .put("viewingMode", state.viewingMode)
                .put("includeTurningPointsInShort", state.includeTurningPointsInShort)
                .put("lastPresentedEventSeq", state.lastPresentedEventSeq)
                .put("lastCompletedCueId", state.lastCompletedCueId ?: JSONObject.NULL)
                .put("inProgressCueId", state.inProgressCueId ?: JSONObject.NULL)
                .put("inProgressCueAnchorSimMs", state.inProgressCueAnchorSimMs ?: JSONObject.NULL)
                .put("inProgressCueText", state.inProgressCueText ?: JSONObject.NULL)
                .put("inProgressCueOffsetMs", state.inProgressCueOffsetMs)
                .put("crowdState", state.crowdState)
                .put("lastTransitionSimTimeMs", state.lastTransitionSimTimeMs)
                .put("lastCrowdRiseSimTimeMs", state.lastCrowdRiseSimTimeMs)
                .put("unpresentedEventsJson", state.unpresentedEventsJson)
                .put("pendingCuesJson", state.pendingCuesJson)
                .put("presentationStateJson", state.presentationStateJson)

        private fun deserializePlaybackState(obj: JSONObject): PlaybackCheckpointState =
            PlaybackCheckpointState(
                playbackSimTimeMs = obj.getLong("playbackSimTimeMs"),
                committedRustSimTimeMs = obj.getLong("committedRustSimTimeMs"),
                playbackRate = obj.getDouble("playbackRate").toFloat(),
                paused = obj.getBoolean("paused"),
                viewingMode = obj.getString("viewingMode"),
                includeTurningPointsInShort = obj.getBoolean("includeTurningPointsInShort"),
                lastPresentedEventSeq = obj.getInt("lastPresentedEventSeq"),
                lastCompletedCueId = obj.optNullableString("lastCompletedCueId"),
                inProgressCueId = obj.optNullableString("inProgressCueId"),
                inProgressCueAnchorSimMs = obj.optNullableLong("inProgressCueAnchorSimMs"),
                inProgressCueText = obj.optNullableString("inProgressCueText"),
                inProgressCueOffsetMs = obj.getLong("inProgressCueOffsetMs"),
                crowdState = obj.getString("crowdState"),
                lastTransitionSimTimeMs = obj.getLong("lastTransitionSimTimeMs"),
                lastCrowdRiseSimTimeMs = obj.getLong("lastCrowdRiseSimTimeMs"),
                unpresentedEventsJson = obj.optString("unpresentedEventsJson", "[]"),
                pendingCuesJson = obj.optString("pendingCuesJson", "[]"),
                presentationStateJson = obj.getString("presentationStateJson")
            )

        private fun canonicalPayloadBytes(
            rustCheckpointJson: String,
            androidPlaybackJson: JSONObject
        ): ByteArray = JSONObject()
            .put("rustCheckpointJson", rustCheckpointJson)
            .put("androidPlayback", androidPlaybackJson)
            .toString()
            .toByteArray(Charsets.UTF_8)

        /** Length-prefixed SHA-256 across all persisted envelope and playback fields. */
        private fun computeIntegritySha256(
            envelope: MatchCheckpointEnvelope,
            androidPlaybackJson: JSONObject
        ): String {
            val digest = MessageDigest.getInstance("SHA-256")
            val fields = listOf(
                envelope.checkpointSchemaVersion.toString(),
                envelope.engineVersion,
                envelope.protocolVersion.toString(),
                envelope.eventSchemaVersion.toString(),
                envelope.rulesetId,
                envelope.rulesetVersion.toString(),
                envelope.timingModelVersion.toString(),
                envelope.matchId,
                envelope.seed.toString(),
                envelope.rustCheckpointJson,
                androidPlaybackJson.toString()
            )
            for (field in fields) {
                val bytes = field.toByteArray(Charsets.UTF_8)
                digest.update(ByteBuffer.allocate(4).putInt(bytes.size).array())
                digest.update(bytes)
            }
            return digest.digest().joinToString("") { "%02x".format(it) }
        }
    }

    data class MatchCheckpointEnvelope(
        val checkpointSchemaVersion: Int = CHECKPOINT_SCHEMA_VERSION,
        val engineVersion: String = DEFAULT_ENGINE_VERSION,
        val protocolVersion: Int = DEFAULT_PROTOCOL_VERSION,
        val eventSchemaVersion: Int = DEFAULT_EVENT_SCHEMA_VERSION,
        val rulesetId: String = DEFAULT_RULESET_ID,
        val rulesetVersion: Int = DEFAULT_RULESET_VERSION,
        val timingModelVersion: Int = DEFAULT_TIMING_MODEL_VERSION,
        val matchId: String,
        val seed: Long,
        val rustCheckpointJson: String,
        val androidPlayback: PlaybackCheckpointState
    )

    private val dir = File(context.applicationContext.filesDir, CHECKPOINT_DIR_NAME).apply {
        if (!exists() && !mkdirs()) throw IOException("Cannot create checkpoint directory")
        if (!isDirectory) throw IOException("Checkpoint path is not a directory")
    }

    private fun checkpointFile(matchId: String): File {
        require(matchId.isNotBlank()) { "Blank matchId" }
        val hash = sha256Hex(matchId.toByteArray(Charsets.UTF_8))
        return File(dir, "$FILE_PREFIX$hash$FILE_SUFFIX")
    }

    private fun previousFile(file: File): File = File(file.path + ".previous")

    private fun writeAtomic(file: File, bytes: ByteArray, isCurrent: () -> Boolean = { true }): Boolean {
        val atomic = AtomicFile(file)
        val stream = atomic.startWrite()
        try {
            stream.write(bytes)
            stream.flush()
            stream.fd.sync()
            if (!isCurrent()) {
                atomic.failWrite(stream)
                return false
            }
            atomic.finishWrite(stream)
            return true
        } catch (error: Exception) {
            atomic.failWrite(stream)
            throw error
        }
    }

    private fun validateEnvelope(envelope: MatchCheckpointEnvelope) {
        require(envelope.checkpointSchemaVersion == CHECKPOINT_SCHEMA_VERSION) {
            "Unsupported checkpointSchemaVersion=${envelope.checkpointSchemaVersion}"
        }
        require(envelope.matchId.isNotBlank()) { "Blank matchId" }
        require(envelope.engineVersion.isNotBlank()) { "Blank engineVersion" }
        require(envelope.protocolVersion > 0 && envelope.eventSchemaVersion > 0) {
            "Invalid protocolVersion or eventSchemaVersion"
        }
        require(
            envelope.rulesetId.isNotBlank() &&
                envelope.rulesetVersion > 0 &&
                envelope.timingModelVersion > 0
        ) {
            "Invalid ruleset or timing model version"
        }
        require(envelope.rustCheckpointJson.isNotBlank()) { "Blank rustCheckpointJson" }
        val rustObj = JSONObject(envelope.rustCheckpointJson)
        require(rustObj.getLong("simTimeMs") == envelope.androidPlayback.committedRustSimTimeMs) {
            "Rust/Android checkpoint frontiers differ"
        }
        require(envelope.androidPlayback.playbackSimTimeMs <= envelope.androidPlayback.committedRustSimTimeMs)
        val rustMatchId = rustObj.optString("matchId", "")
        if (rustMatchId.isNotBlank()) {
            require(rustMatchId == envelope.matchId) {
                "Rust checkpoint matchId '$rustMatchId' differs from envelope matchId '${envelope.matchId}'"
            }
        }
    }

    /**
     * Atomically writes or updates the active match checkpoint with `AtomicFile` + `fd.sync()`
     * and dual-layer checksums (`integritySha256` + `payloadCrc32`).
     */
    fun saveAtomic(envelope: MatchCheckpointEnvelope) {
        check(saveAtomicIfCurrent(envelope) { true })
    }

    /** Reject a stale worker before writing and again before publishing the primary file. */
    fun saveAtomicIfCurrent(envelope: MatchCheckpointEnvelope, isCurrent: () -> Boolean): Boolean {
        if (!isCurrent()) return false
        validateEnvelope(envelope)
        val playbackJson = serializePlaybackState(envelope.androidPlayback)
        val payloadCrc32 = crc32Value(canonicalPayloadBytes(envelope.rustCheckpointJson, playbackJson))
        val integritySha256 = computeIntegritySha256(envelope, playbackJson)

        val root = JSONObject()
            .put("checkpointSchemaVersion", envelope.checkpointSchemaVersion)
            .put("engineVersion", envelope.engineVersion)
            .put("protocolVersion", envelope.protocolVersion)
            .put("eventSchemaVersion", envelope.eventSchemaVersion)
            .put("rulesetId", envelope.rulesetId)
            .put("rulesetVersion", envelope.rulesetVersion)
            .put("timingModelVersion", envelope.timingModelVersion)
            .put("matchId", envelope.matchId)
            .put("seed", envelope.seed)
            .put("rustCheckpointJson", envelope.rustCheckpointJson)
            .put("androidPlayback", playbackJson)
            .put("payloadCrc32", payloadCrc32)
            .put("integritySha256", integritySha256)

        val destination = checkpointFile(envelope.matchId)
        val bytes = root.toString().toByteArray(Charsets.UTF_8)
        return synchronized(fileLock) {
            if (!isCurrent()) return@synchronized false
            // Keep the last fully verified generation. Never replace a good
            // previous generation with a corrupt primary file.
            if (readVerifiedFile(destination) != null) {
                writeAtomic(previousFile(destination), AtomicFile(destination).readFully())
            }
            writeAtomic(destination, bytes, isCurrent)
        }
    }

    /**
     * Convenience overload for saving an active checkpoint from individual components.
     */
    fun saveAtomic(
        matchId: String,
        seed: Long,
        rustCheckpointJson: String,
        androidPlayback: PlaybackCheckpointState,
        engineVersion: String = DEFAULT_ENGINE_VERSION,
        protocolVersion: Int = DEFAULT_PROTOCOL_VERSION,
        eventSchemaVersion: Int = DEFAULT_EVENT_SCHEMA_VERSION,
        rulesetId: String = DEFAULT_RULESET_ID,
        rulesetVersion: Int = DEFAULT_RULESET_VERSION,
        timingModelVersion: Int = DEFAULT_TIMING_MODEL_VERSION
    ) {
        saveAtomic(
            MatchCheckpointEnvelope(
                checkpointSchemaVersion = CHECKPOINT_SCHEMA_VERSION,
                engineVersion = engineVersion,
                protocolVersion = protocolVersion,
                eventSchemaVersion = eventSchemaVersion,
                rulesetId = rulesetId,
                rulesetVersion = rulesetVersion,
                timingModelVersion = timingModelVersion,
                matchId = matchId,
                seed = seed,
                rustCheckpointJson = rustCheckpointJson,
                androidPlayback = androidPlayback
            )
        )
    }

    private fun readVerifiedFile(file: File): MatchCheckpointEnvelope? {
        return try {
            val bytes = AtomicFile(file).readFully()
            val root = JSONObject(String(bytes, Charsets.UTF_8))
            val schemaVersion = root.optInt("checkpointSchemaVersion", -1)
            if (schemaVersion != CHECKPOINT_SCHEMA_VERSION) return null

            val playbackObj = root.getJSONObject("androidPlayback")
            val playbackState = deserializePlaybackState(playbackObj)
            val envelope = MatchCheckpointEnvelope(
                checkpointSchemaVersion = schemaVersion,
                engineVersion = root.getString("engineVersion"),
                protocolVersion = root.getInt("protocolVersion"),
                eventSchemaVersion = root.getInt("eventSchemaVersion"),
                rulesetId = root.getString("rulesetId"),
                rulesetVersion = root.getInt("rulesetVersion"),
                timingModelVersion = root.getInt("timingModelVersion"),
                matchId = root.getString("matchId"),
                seed = root.getLong("seed"),
                rustCheckpointJson = root.getString("rustCheckpointJson"),
                androidPlayback = playbackState
            )
            validateEnvelope(envelope)
            val expectedFile = checkpointFile(envelope.matchId)
            if (file.name != expectedFile.name && file.name != previousFile(expectedFile).name) return null

            val canonicalPlayback = serializePlaybackState(playbackState)
            val expectedCrc = crc32Value(canonicalPayloadBytes(envelope.rustCheckpointJson, canonicalPlayback))
            if (!root.has("payloadCrc32") || root.getLong("payloadCrc32") != expectedCrc) return null

            val expectedSha = computeIntegritySha256(envelope, canonicalPlayback)
            if (root.optString("integritySha256") != expectedSha) return null

            envelope
        } catch (_: Exception) {
            null
        }
    }

    /**
     * Loads and verifies an active checkpoint for [matchId], returning `null` if missing
     * or if either `payloadCrc32` or `integritySha256` verification fails.
     */
    fun loadVerified(matchId: String): MatchCheckpointEnvelope? = synchronized(fileLock) {
        if (matchId.isBlank()) return@synchronized null
        val file = checkpointFile(matchId)
        (readVerifiedFile(file) ?: readVerifiedFile(previousFile(file)))?.takeIf { it.matchId == matchId }
    }

    /**
     * Lists all valid, integrity-verified active match checkpoints sorted by `matchId`.
     */
    fun listActiveCheckpoints(): List<MatchCheckpointEnvelope> = synchronized(fileLock) {
        dir.listFiles()?.asSequence()
            ?.filter { it.name.startsWith(FILE_PREFIX) && (it.name.endsWith(FILE_SUFFIX) || it.name.endsWith("$FILE_SUFFIX.previous")) }
            ?.map { File(dir, it.name.removeSuffix(".previous")) }
            ?.distinctBy { it.name }
            ?.mapNotNull { readVerifiedFile(it) ?: readVerifiedFile(previousFile(it)) }
            ?.sortedBy { it.matchId }
            ?.toList()
            .orEmpty()
    }

    /**
     * Deletes the active checkpoint for [matchId] (must be called when the match reaches `FULL_TIME`).
     */
    fun deleteCheckpoint(matchId: String): Boolean = deleteCheckpointIfCurrent(matchId) { true }

    fun deleteCheckpointIfCurrent(matchId: String, isCurrent: () -> Boolean): Boolean = synchronized(fileLock) {
        if (!isCurrent() || matchId.isBlank()) return@synchronized false
        val file = checkpointFile(matchId)
        val previous = previousFile(file)
        val existed = file.exists() || previous.exists()
        AtomicFile(file).delete()
        AtomicFile(previous).delete()
        existed && !file.exists() && !previous.exists()
    }
}
