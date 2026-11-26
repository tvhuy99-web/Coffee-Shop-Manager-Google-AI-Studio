package pl.mkrstudio.truefootball3.match_playback

import pl.mkrstudio.truefootball3.lab.*

import android.content.Context
import android.util.AtomicFile
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.io.IOException
import java.nio.ByteBuffer
import java.security.MessageDigest

/**
 * Crash-safe, immutable, uniquely keyed Seed-Replay archives.
 *
 * 1. Filenames are SHA-256(matchInstanceId) and cannot collide after lossy
 *    sanitization or escape the application's private file directory.
 * 2. AtomicFile writes to a temporary new file with fsync and restores old data on failure.
 * 3. An existing archive is NEVER silently replaced by a different match.
 * 4. An integrity digest covers the complete input, result, accepted commands,
 *    seed, engine/protocol/event-schema/ruleset/timing-model versions, and event timeline fingerprint.
 * 5. Legacy archives (schema v2 or engine v4.0.0) surface an explicit [LegacyVersionNotice]
 *    instead of a misleading fingerprint mismatch error.
 *
 * Finalized matches only. Mid-match crash recovery is handled by [MatchCheckpointStore].
 */
class LabGlobalSeedReplayStore(context: Context) {
    companion object {
        const val SCHEMA_VERSION = 3
        const val LEGACY_SCHEMA_VERSION_V2 = 2
        const val CURRENT_ENGINE_VERSION = "v4.1.1-des"
        const val LEGACY_ENGINE_VERSION_V4_0 = "v4.0.0"
        const val CURRENT_PROTOCOL_VERSION = 5
        const val CURRENT_EVENT_SCHEMA_VERSION = 5
        const val DEFAULT_RULESET_ID = "ifab_league_standard"
        const val DEFAULT_RULESET_VERSION = 1
        const val DEFAULT_TIMING_MODEL_VERSION = 1

        private val fileLock = Any()

        private fun sha256(data: ByteArray): String =
            MessageDigest.getInstance("SHA-256").digest(data)
                .joinToString("") { "%02x".format(it) }

        /** Length-prefix each field to avoid ambiguous concatenations. */
        private fun recordDigest(record: StoredMatchRecord): String {
            val digest = MessageDigest.getInstance("SHA-256")
            val fields = listOf(
                SCHEMA_VERSION.toString(),
                record.matchId,
                record.seed.toString(),
                record.engineVersion,
                record.protocolVersion.toString(),
                record.eventSchemaVersion.toString(),
                record.rulesetId,
                record.rulesetVersion.toString(),
                record.timingModelVersion.toString(),
                record.inputJson,
                record.summaryJson,
                record.commandTimelineJson,
                record.eventTimelineFingerprint
            )
            for (field in fields) {
                val bytes = field.toByteArray(Charsets.UTF_8)
                digest.update(ByteBuffer.allocate(4).putInt(bytes.size).array())
                digest.update(bytes)
            }
            return digest.digest().joinToString("") { "%02x".format(it) }
        }
    }

    private val dir = File(context.applicationContext.filesDir, "v4_global_snapshots").apply {
        if (!exists() && !mkdirs()) throw IOException("Cannot create snapshot directory")
        if (!isDirectory) throw IOException("Snapshot path is not a directory")
    }

    data class StoredMatchRecord(
        val matchId: String,
        val seed: Long,
        val engineVersion: String,
        val inputJson: String,
        val summaryJson: String,
        val commandTimelineJson: String = "[]",
        val eventTimelineFingerprint: String = "",
        val protocolVersion: Int = CURRENT_PROTOCOL_VERSION,
        val eventSchemaVersion: Int = CURRENT_EVENT_SCHEMA_VERSION,
        val rulesetId: String = DEFAULT_RULESET_ID,
        val rulesetVersion: Int = DEFAULT_RULESET_VERSION,
        val timingModelVersion: Int = DEFAULT_TIMING_MODEL_VERSION
    )

    data class LegacyVersionNotice(
        val matchId: String,
        val foundSchemaVersion: Int,
        val currentSchemaVersion: Int,
        val foundEngineVersion: String,
        val currentEngineVersion: String,
        val message: String
    )

    sealed class LoadSnapshotResult {
        data class Success(val record: StoredMatchRecord) : LoadSnapshotResult()
        data class LegacyIncompatible(val notice: LegacyVersionNotice) : LoadSnapshotResult()
        data class NotFound(val matchId: String) : LoadSnapshotResult()
        data class Corrupted(val matchId: String, val reason: String) : LoadSnapshotResult()
    }

    private fun archiveFile(matchId: String): File {
        require(matchId.isNotBlank()) { "Blank match instance ID" }
        val token = sha256(matchId.toByteArray(Charsets.UTF_8))
        return File(dir, "match-$token.json")
    }

    private fun validateRecord(record: StoredMatchRecord) {
        require(record.matchId.isNotBlank() && record.engineVersion.isNotBlank()) { "Incomplete match identity" }
        require(record.eventTimelineFingerprint.isNotBlank()) { "Missing event timeline fingerprint" }
        require(record.protocolVersion > 0 && record.eventSchemaVersion > 0) { "Invalid protocol/event schema version" }
        require(record.rulesetId.isNotBlank() && record.rulesetVersion > 0 && record.timingModelVersion > 0) {
            "Invalid ruleset or timing model metadata"
        }
        val input = JSONObject(record.inputJson)
        require(input.getString("matchId") == record.matchId) { "Snapshot match ID differs from immutable input" }
        require(input.getLong("seed") == record.seed) { "Snapshot seed differs from input" }
        require(input.optString("engineVersion") == record.engineVersion) { "Snapshot engine version differs from input" }
        JSONArray(record.commandTimelineJson)
        JSONObject(record.summaryJson)
    }

    private fun isLegacyEngineVersion(engineVersion: String): Boolean =
        engineVersion == LEGACY_ENGINE_VERSION_V4_0 || engineVersion.startsWith("v4.0.")

    private fun buildLegacyNotice(matchId: String, foundSchema: Int, foundEngine: String): LegacyVersionNotice {
        val msg = "Bản lưu trận '$matchId' được tạo bởi phiên bản cũ " +
            "(schema v$foundSchema, engine $foundEngine) và không tương thích phát lại trực tiếp " +
            "trên động cơ hiện tại (schema v$SCHEMA_VERSION, engine $CURRENT_ENGINE_VERSION). " +
            "Đây là thông báo tương thích phiên bản, không phải lỗi sai lệch fingerprint."
        return LegacyVersionNotice(
            matchId = matchId,
            foundSchemaVersion = foundSchema,
            currentSchemaVersion = SCHEMA_VERSION,
            foundEngineVersion = foundEngine,
            currentEngineVersion = CURRENT_ENGINE_VERSION,
            message = msg
        )
    }

    fun saveSnapshot(record: StoredMatchRecord) {
        validateRecord(record)
        val payload = JSONObject()
            .put("schemaVersion", SCHEMA_VERSION)
            .put("matchId", record.matchId)
            .put("seed", record.seed)
            .put("engineVersion", record.engineVersion)
            .put("protocolVersion", record.protocolVersion)
            .put("eventSchemaVersion", record.eventSchemaVersion)
            .put("rulesetId", record.rulesetId)
            .put("rulesetVersion", record.rulesetVersion)
            .put("timingModelVersion", record.timingModelVersion)
            .put("inputJson", record.inputJson)
            .put("summaryJson", record.summaryJson)
            .put("commandTimelineJson", record.commandTimelineJson)
            .put("eventTimelineFingerprint", record.eventTimelineFingerprint)
            .put("integritySha256", recordDigest(record))
        val destination = archiveFile(record.matchId)
        synchronized(fileLock) {
            if (destination.exists()) {
                when (val existing = readArchiveDetailed(destination, record.matchId)) {
                    is LoadSnapshotResult.Success -> {
                        if (existing.record == record) return // Idempotent save of exactly the same match.
                        throw IOException("Immutable match archive already exists: ${record.matchId}")
                    }
                    is LoadSnapshotResult.LegacyIncompatible -> {
                        throw IOException(existing.notice.message)
                    }
                    else -> {
                        throw IOException("Existing match archive failed integrity check")
                    }
                }
            }
            val atomic = AtomicFile(destination)
            val stream = atomic.startWrite()
            try {
                stream.write(payload.toString().toByteArray(Charsets.UTF_8))
                stream.flush()
                stream.fd.sync()
                atomic.finishWrite(stream)
            } catch (error: Exception) {
                atomic.failWrite(stream)
                throw error
            }
        }
    }

    private fun readArchiveDetailed(file: File, expectedMatchId: String? = null): LoadSnapshotResult {
        val fallbackId = expectedMatchId ?: file.name
        return try {
            val bytes = AtomicFile(file).readFully()
            val obj = JSONObject(String(bytes, Charsets.UTF_8))
            val schemaVer = obj.optInt("schemaVersion", -1)
            val savedMatchId = obj.optString("matchId", fallbackId)
            val savedEngineVersion = obj.optString("engineVersion", "")

            if (schemaVer == LEGACY_SCHEMA_VERSION_V2 ||
                (schemaVer in 1 until SCHEMA_VERSION) ||
                isLegacyEngineVersion(savedEngineVersion)
            ) {
                return LoadSnapshotResult.LegacyIncompatible(
                    buildLegacyNotice(
                        matchId = savedMatchId.ifBlank { fallbackId },
                        foundSchema = schemaVer,
                        foundEngine = savedEngineVersion.ifBlank { LEGACY_ENGINE_VERSION_V4_0 }
                    )
                )
            }

            if (schemaVer != SCHEMA_VERSION) {
                return LoadSnapshotResult.Corrupted(
                    savedMatchId,
                    "Unsupported snapshot schemaVersion=$schemaVer (expected $SCHEMA_VERSION)"
                )
            }

            val record = StoredMatchRecord(
                matchId = obj.getString("matchId"),
                seed = obj.getLong("seed"),
                engineVersion = obj.getString("engineVersion"),
                inputJson = obj.getString("inputJson"),
                summaryJson = obj.getString("summaryJson"),
                commandTimelineJson = obj.getString("commandTimelineJson"),
                eventTimelineFingerprint = obj.getString("eventTimelineFingerprint"),
                protocolVersion = obj.getInt("protocolVersion"),
                eventSchemaVersion = obj.getInt("eventSchemaVersion"),
                rulesetId = obj.getString("rulesetId"),
                rulesetVersion = obj.getInt("rulesetVersion"),
                timingModelVersion = obj.getInt("timingModelVersion")
            )
            validateRecord(record)
            if (recordDigest(record) != obj.optString("integritySha256")) {
                return LoadSnapshotResult.Corrupted(record.matchId, "Snapshot integritySha256 mismatch")
            }
            if (file.name != archiveFile(record.matchId).name) {
                return LoadSnapshotResult.Corrupted(record.matchId, "Snapshot filename hash mismatch")
            }
            if (expectedMatchId != null && record.matchId != expectedMatchId) {
                return LoadSnapshotResult.Corrupted(expectedMatchId, "Snapshot matchId mismatch")
            }
            LoadSnapshotResult.Success(record)
        } catch (e: Exception) {
            LoadSnapshotResult.Corrupted(fallbackId, e.message ?: "Malformed snapshot archive")
        }
    }

    fun loadSnapshotWithStatus(matchId: String): LoadSnapshotResult = synchronized(fileLock) {
        val file = archiveFile(matchId)
        if (!file.exists()) return@synchronized LoadSnapshotResult.NotFound(matchId)
        readArchiveDetailed(file, matchId)
    }

    fun loadSnapshot(matchId: String): StoredMatchRecord? =
        (loadSnapshotWithStatus(matchId) as? LoadSnapshotResult.Success)?.record

    fun listStoredMatchIds(): List<String> = synchronized(fileLock) {
        dir.listFiles()?.asSequence()
            ?.filter { it.name.startsWith("match-") && it.name.endsWith(".json") }
            ?.mapNotNull { (readArchiveDetailed(it) as? LoadSnapshotResult.Success)?.record?.matchId }
            ?.sorted()?.toList().orEmpty()
    }

    /** Replays commands at their original Rust simulation beats, never TTS time. */
    fun replayFullEvent(matchId: String): String {
        val record = when (val loaded = loadSnapshotWithStatus(matchId)) {
            is LoadSnapshotResult.Success -> loaded.record
            is LoadSnapshotResult.LegacyIncompatible -> {
                val n = loaded.notice
                return JSONObject()
                    .put("ok", false)
                    .put("code", "LEGACY_VERSION_INCOMPATIBLE")
                    .put("error", n.message)
                    .put(
                        "legacyVersionNotice",
                        JSONObject()
                            .put("matchId", n.matchId)
                            .put("foundSchemaVersion", n.foundSchemaVersion)
                            .put("currentSchemaVersion", n.currentSchemaVersion)
                            .put("foundEngineVersion", n.foundEngineVersion)
                            .put("currentEngineVersion", n.currentEngineVersion)
                            .put("message", n.message)
                    )
                    .toString()
            }
            is LoadSnapshotResult.NotFound -> {
                return JSONObject()
                    .put("ok", false)
                    .put("code", "SNAPSHOT_NOT_FOUND")
                    .put("error", "Snapshot not found for matchId=$matchId")
                    .toString()
            }
            is LoadSnapshotResult.Corrupted -> {
                return JSONObject()
                    .put("ok", false)
                    .put("code", "SNAPSHOT_CORRUPTED")
                    .put("error", "Snapshot failed integrity verification: ${loaded.reason}")
                    .toString()
            }
        }

        if (record.protocolVersion != CURRENT_PROTOCOL_VERSION ||
            record.eventSchemaVersion != CURRENT_EVENT_SCHEMA_VERSION ||
            record.rulesetVersion != DEFAULT_RULESET_VERSION ||
            record.timingModelVersion != DEFAULT_TIMING_MODEL_VERSION
        ) {
            val msg = "Bản lưu trận '${record.matchId}' không khớp phiên bản giao thức/luật hiện tại " +
                "(saved protocol=v${record.protocolVersion}, eventSchema=v${record.eventSchemaVersion}, " +
                "ruleset=${record.rulesetId}@v${record.rulesetVersion}, timing=v${record.timingModelVersion}; " +
                "current protocol=v$CURRENT_PROTOCOL_VERSION, eventSchema=v$CURRENT_EVENT_SCHEMA_VERSION, " +
                "rulesetVersion=v$DEFAULT_RULESET_VERSION, timing=v$DEFAULT_TIMING_MODEL_VERSION). " +
                "Đây là khác biệt phiên bản, không phải sai lệch fingerprint."
            return JSONObject()
                .put("ok", false)
                .put("code", "VERSION_INCOMPATIBLE")
                .put("error", msg)
                .toString()
        }

        val inputObj = JSONObject(record.inputJson)
        inputObj.put("simulationMode", LabSimulationMode.FULL_EVENT.name)
        val timeline = JSONArray(record.commandTimelineJson)
        val raw = if (timeline.length() > 0) {
            NativeBridge.replayMatchV4(inputObj.toString(), timeline.toString())
        } else {
            NativeBridge.simulateMatchV4(inputObj.toString())
        }
        val result = try { JSONObject(raw) } catch (_: Exception) {
            return """{"ok":false,"error":"Invalid replay response"}"""
        }
        if (!result.optBoolean("ok", false)) return raw
        if (result.optString("eventTimelineFingerprint") != record.eventTimelineFingerprint) {
            return """{"ok":false,"error":"Replay fingerprint mismatch"}"""
        }
        return raw
    }
}
