package pl.mkrstudio.truefootball3.lab
import org.json.JSONObject

object LabMatchInputFactory { const val ENGINE_VERSION_V4 = "v4.0.0" }
enum class LabSimulationMode { FULL_EVENT, FAST_BATCH }
enum class LabEngineVersion { V3, V4 }
object LabLogStore { fun log(kind: String, message: String) {} }
object NativeBridge {
    var lastV4Command = ""
    var expectedFingerprint = "fp-123"
    fun isFullEngineReady() = true
    fun isV4InteractiveReady() = true
    fun nativeCreateSession(input: String) = 66L
    fun createV4Session(input: String) = 77L
    fun nativeSubmitCommand(handle: Long, payload: String) = 1
    fun submitV4Command(handle: Long, payload: String): Boolean {
        lastV4Command = payload
        return true
    }
    fun nativeAdvanceSession(handle: Long, maxSteps: Int) = JSONObject().put("ok", true).toString()
    fun advanceV4Session(handle: Long, maxSteps: Int) = JSONObject().put("ok", true).toString()
    fun nativeFinishSession(handle: Long) = JSONObject().put("ok", true).toString()
    fun finishV4Session(handle: Long) = JSONObject().put("ok", true).toString()
    fun nativeDestroySession(handle: Long) {}
    fun destroyV4Session(handle: Long) {}
    fun replayMatchV4(input: String, timeline: String): String = simulateMatchV4(input)
    fun simulateMatchV4(input: String) = JSONObject().put("ok", true)
        .put("eventTimelineFingerprint", expectedFingerprint).toString()
}
