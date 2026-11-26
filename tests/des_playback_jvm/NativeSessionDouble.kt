package pl.mkrstudio.truefootball3.match_playback
import org.json.*
class LabRustLiveSession(private val advance:(Long)->JSONObject){var isOpen=true;val targets=mutableListOf<Long>();fun advanceUntilMs(target:Long,maxSteps:Int):String{targets.add(target);return advance(target).toString()};fun startNextPeriod()=JSONObject().put("ok",false).toString();fun submitCommandJson(s:String)=JSONObject().put("ok",true).toString();fun finish()=JSONObject().put("ok",true);fun close(){isOpen=false};fun exportCheckpointJson()=JSONObject().put("simTimeMs",0L).toString()}
