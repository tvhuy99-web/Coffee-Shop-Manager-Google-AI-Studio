package pl.mkrstudio.truefootball3.lab

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
 * 2. AtomicFile writes to a temporary new file and restores old data on failure.
 * 3. An existing archive is NEVER silently replaced by a different match.
 * 4. An integrity digest covers the complete input, result, accepted commands,
 *    seed, engine version and event timeline fingerprint.
 *
 * Finalized matches only. Mid-match crash recovery requires separate durable
 * command/checkpoint journaling and is not promised by this class.
 */
class LabGlobalSeedReplayStore(context: Context) {
    companion object {
        private const val SCHEMA_VERSION = 2
        private val fileLock = Any()

        private fun sha256(data: ByteArray): String =
            MessageDigest.getInstance("SHA-256").digest(data)
                .joinToString("") { "%02x".format(it) }

        /** Length-prefix each field to avoid ambiguous concatenations. */
        private fun recordDigest(record: StoredMatchRecord): String {
            val digest = MessageDigest.getInstance("SHA-256")
            val fields = listOf(
                SCHEMA_VERSION.toString(), record.matchId, record.seed.toString(),
                record.engineVersion, record.inputJson, record.summaryJson,
                record.commandTimelineJson, record.eventTimelineFingerprint
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
        val eventTimelineFingerprint: String = ""
    )

    private fun archiveFile(matchId: String): File {
        require(matchId.isNotBlank()) { "Blank match instance ID" }
        val token = sha256(matchId.toByteArray(Charsets.UTF_8))
        return File(dir, "match-$token.json")
    }

    private fun validateRecord(record: StoredMatchRecord) {
        require(record.matchId.isNotBlank() && record.engineVersion.isNotBlank()) { "Incomplete match identity" }
        require(record.eventTimelineFingerprint.isNotBlank()) { "Missing event timeline fingerprint" }
        val input = JSONObject(record.inputJson)
        require(input.getString("matchId") == record.matchId) { "Snapshot match ID differs from immutable input" }
        require(input.getLong("seed") == record.seed) { "Snapshot seed differs from input" }
        require(input.optString("engineVersion") == record.engineVersion) { "Snapshot engine version differs from input" }
        JSONArray(record.commandTimelineJson)
        JSONObject(record.summaryJson)
    }

    fun saveSnapshot(record: StoredMatchRecord) {
        validateRecord(record)
        val payload = JSONObject()
            .put("schemaVersion", SCHEMA_VERSION)
            .put("matchId", record.matchId)
            .put("seed", record.seed)
            .put("engineVersion", record.engineVersion)
            .put("inputJson", record.inputJson)
            .put("summaryJson", record.summaryJson)
            .put("commandTimelineJson", record.commandTimelineJson)
            .put("eventTimelineFingerprint", record.eventTimelineFingerprint)
            .put("integritySha256", recordDigest(record))
        val destination = archiveFile(record.matchId)
        synchronized(fileLock) {
            if (destination.exists()) {
                val previous = readArchive(destination)
                    ?: throw IOException("Existing match archive failed integrity check")
                if (previous == record) return // Idempotent save of exactly the same match.
                throw IOException("Immutable match archive already exists: ${record.matchId}")
            }
            val atomic = AtomicFile(destination)
            val stream = atomic.startWrite()
            try {
                stream.write(payload.toString().toByteArray(Charsets.UTF_8))
                atomic.finishWrite(stream)
            } catch (error: Exception) {
                atomic.failWrite(stream)
                throw error
            }
        }
    }

    private fun readArchive(file: File): StoredMatchRecord? {
        return try {
        val bytes = AtomicFile(file).readFully()
        val obj = JSONObject(String(bytes, Charsets.UTF_8))
        if (obj.optInt("schemaVersion", -1) != SCHEMA_VERSION) return null
        val record = StoredMatchRecord(
            matchId = obj.getString("matchId"),
            seed = obj.getLong("seed"),
            engineVersion = obj.getString("engineVersion"),
            inputJson = obj.getString("inputJson"),
            summaryJson = obj.getString("summaryJson"),
            commandTimelineJson = obj.getString("commandTimelineJson"),
            eventTimelineFingerprint = obj.getString("eventTimelineFingerprint")
        )
        validateRecord(record)
        if (recordDigest(record) != obj.optString("integritySha256")) return null
        if (file.name != archiveFile(record.matchId).name) return null
        record
        } catch (_: Exception) {
            null
        }
    }

    fun loadSnapshot(matchId: String): StoredMatchRecord? = synchronized(fileLock) {
        val file = archiveFile(matchId)
        if (!file.exists()) return@synchronized null
        readArchive(file)?.takeIf { it.matchId == matchId }
    }

    fun listStoredMatchIds(): List<String> = synchronized(fileLock) {
        dir.listFiles()?.asSequence()
            ?.filter { it.name.startsWith("match-") && it.name.endsWith(".json") }
            ?.mapNotNull { readArchive(it)?.matchId }
            ?.sorted()?.toList().orEmpty()
    }

    /** Replays commands at their original Rust simulation beats, never TTS time. */
    fun replayFullEvent(matchId: String): String {
        val record = loadSnapshot(matchId)
            ?: return """{"ok":false,"error":"Snapshot missing or failed integrity verification"}"""
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
