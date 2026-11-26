package pl.mkrstudio.truefootball3.lab
import org.json.JSONObject

object LabMatchInputFactory { const val ENGINE_VERSION_V4 = "v4.0.0" }
enum class LabSimulationMode { FULL_EVENT, FAST_BATCH }
enum class LabEngineVersion { V4 }
object LabLogStore { fun log(kind: String, message: String) {} }
object NativeBridge {
    var lastV4Command = ""
    var expectedFingerprint = "fp-123"
    fun isV4InteractiveReady() = true
    // Test double: the native DES behavior is exercised in Rust and the separate
    // controller JVM suite. Unexpected calls here fail, rather than fake success.
    fun isV4DesReady() = true
    fun importV4Checkpoint(checkpointJson: String): Long = error("Unexpected native checkpoint import in P1 smoke")
    fun advanceV4SessionUntilMs(handle: Long, targetSimTimeMs: Long, maxStepsGuard: Int): String = error("Unexpected native DES advance in P1 smoke")
    fun startNextPeriod(handle: Long): String = error("Unexpected native period start in P1 smoke")
    fun exportV4Checkpoint(handle: Long): String = error("Unexpected native checkpoint export in P1 smoke")
    fun submitV4CommandJson(handle: Long, commandJson: String): String = error("Unexpected native raw command in P1 smoke")
    fun createV4Session(input: String) = 77L
    fun submitV4Command(handle: Long, payload: String): Boolean {
        lastV4Command = payload
        return true
    }
    fun advanceV4Session(handle: Long, maxSteps: Int) = JSONObject().put("ok", true).toString()
    fun finishV4Session(handle: Long) = JSONObject().put("ok", true).toString()
    fun destroyV4Session(handle: Long) {}
    fun replayMatchV4(input: String, timeline: String): String = simulateMatchV4(input)
    fun simulateMatchV4(input: String) = JSONObject().put("ok", true)
        .put("eventTimelineFingerprint", expectedFingerprint).toString()
}
