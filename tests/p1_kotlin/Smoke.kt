package pl.mkrstudio.truefootball3.lab

import android.content.Context
import org.json.JSONObject
import java.io.File
import java.nio.file.Files

fun main() {
    val base = Files.createTempDirectory("v4-p1-snapshot-test").toFile()
    val store = LabGlobalSeedReplayStore(Context(base))
    fun record(id: String, fp: String = "fp-123") = LabGlobalSeedReplayStore.StoredMatchRecord(
        matchId = id, seed = 12345L, engineVersion = "v4.0.0",
        inputJson = JSONObject().put("matchId", id).put("seed", 12345L)
            .put("engineVersion", "v4.0.0").toString(),
        summaryJson = JSONObject().put("test", "summary").toString(),
        eventTimelineFingerprint = fp
    )
    var checks = 0
    fun assertCase(cond: Boolean, detail: String) { checks++; check(cond) { detail } }

    val first = record("Season2/round01:home-away")
    store.saveSnapshot(first)
    assertCase(store.loadSnapshot(first.matchId) == first, "snapshot must round-trip")
    store.saveSnapshot(first) // safe idempotent retry
    assertCase(store.listStoredMatchIds() == listOf(first.matchId), "raw match ID must survive file hashing")
    var collisionRefused = false
    try { store.saveSnapshot(first.copy(eventTimelineFingerprint = "different")) }
    catch (_: java.io.IOException) { collisionRefused = true }
    assertCase(collisionRefused, "different result must never overwrite same fixture ID")

    val second = record("Season2_round01:home-away")
    store.saveSnapshot(second)
    assertCase(store.listStoredMatchIds().size == 2, "former sanitization collisions must remain distinct")
    val replay = JSONObject(store.replayFullEvent(first.matchId))
    assertCase(replay.optBoolean("ok"), "matching fingerprint should allow replay")
    NativeBridge.expectedFingerprint = "WRONG"
    val mismatch = store.replayFullEvent(first.matchId)
    assertCase(mismatch.contains("Replay fingerprint mismatch"), "mismatched replay fingerprint must be blocked")
    NativeBridge.expectedFingerprint = "fp-123"

    // Deliberate truncation should be detected, not quietly interpreted as a match.
    val archive = File(base, "v4_global_snapshots").listFiles()!!.first()
    archive.writeText("corrupted archive")
    assertCase(store.listStoredMatchIds().size == 1, "corrupted archive excluded")

    val live = LabRustLiveSession.open(JSONObject().put("homeTeam", JSONObject()).toString(), LabEngineVersion.V4)!!
    assertCase(live.submitSubstitutions(900, "HOME", listOf("H07" to "SUB1", "H08" to "SUB2")),
        "two changes must submit as one grouped command")
    val payload = JSONObject(NativeBridge.lastV4Command)
    assertCase(payload.getString("type") == "SUBSTITUTE_BATCH", "batch command type")
    assertCase(payload.optJSONArray("changes")!!.length() == 2, "two changes use one payload")
    assertCase(!live.submitSubstitutions(900, "HOME", listOf("H07" to "S1", "H07" to "S2")),
        "duplicate outgoing player must be rejected before JNI")
    live.close()
    println("PASS: $checks real Kotlin/JVM snapshot and grouped-command assertions (fake JSON/Android runtime)")
    base.deleteRecursively()
}
