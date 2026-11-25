package pl.mkrstudio.truefootball3.lab
import org.json.*
object LabLogStore{fun log(k:String,m:String){};fun error(k:String,m:String,e:Throwable){throw AssertionError(m,e)}}
class LabRustLiveSession(private val advance:(Long)->JSONObject){var isOpen=true;val targets=mutableListOf<Long>();fun advanceUntilMs(target:Long,maxSteps:Int):String{targets.add(target);return advance(target).toString()};fun startNextPeriod()=JSONObject().put("ok",false).toString();fun submitCommandJson(s:String)=JSONObject().put("ok",true).toString();fun finish()=JSONObject().put("ok",true);fun close(){isOpen=false};fun exportCheckpointJson()=JSONObject().put("simTimeMs",0L).toString()}
class LabAudioDirector{enum class CrowdState{CALM};data class DirectorState(val crowdState:CrowdState=CrowdState.CALM,val lastTransitionSimTimeMs:Long=0,val lastCrowdRiseSimTimeMs:Long=0);fun exportState()=DirectorState();fun restoreState(s:DirectorState){};fun onFastForwardSkip(t:Long){};fun onEvent(kind:LabEventType,side:LabTeamSide?,minute:Int,goalDiffAbs:Int,simTimeMs:Long,isResumedReplay:Boolean=false){}}

// Only offline sequence playback touches this settings double; the filter is real production code.
class SimulationSettingsStore(context: android.content.Context) {
    data class Settings(val syncMatchSpeedWithTts:Boolean=true)
    fun load()=Settings()
}
