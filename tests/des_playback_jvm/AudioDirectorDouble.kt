package pl.mkrstudio.truefootball3.match_audio
import pl.mkrstudio.truefootball3.lab.*
class LabAudioDirector{enum class CrowdState{CALM};data class DirectorState(val crowdState:CrowdState=CrowdState.CALM,val lastTransitionSimTimeMs:Long=0,val lastCrowdRiseSimTimeMs:Long=0);fun exportState()=DirectorState();fun restoreState(s:DirectorState){};fun onFastForwardSkip(t:Long){};fun onEvent(kind:LabEventType,side:LabTeamSide?,minute:Int,goalDiffAbs:Int,simTimeMs:Long,isResumedReplay:Boolean=false){}}
