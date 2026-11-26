package pl.mkrstudio.truefootball3.lab
import android.content.Context
import pl.mkrstudio.truefootball3.presentation.PresentationSessionGate
import android.os.TestClock
import pl.mkrstudio.truefootball3.audio.MatchTtsManager
import java.nio.file.Files
import java.util.concurrent.AbstractExecutorService
import java.util.concurrent.TimeUnit
import org.json.*
class InlineExecutor:AbstractExecutorService(){private var stopped=false;override fun execute(r:Runnable)=r.run();override fun shutdown(){stopped=true};override fun shutdownNow()=mutableListOf<Runnable>();override fun isShutdown()=stopped;override fun isTerminated()=stopped;override fun awaitTermination(t:Long,u:TimeUnit)=true}
fun event(i:Int,ms:Long)=NarrationScheduler.SimulationEvent(i,ms,1,ms,1,"1",LabEventType.GOAL,LabTeamSide.HOME,i,0,"Goal $i")
fun main(){val dir=Files.createTempDirectory("des-qa").toFile();val ctx=Context(dir);try{
// Use the production filter, including an own goal with NONE tier to ensure
// short highlights depend on kind, not a conveniently assigned GOAL_KEY tier.
fun raw(kind:String,seq:Int,tier:String="NONE")=JSONObject().put("kind",kind).put("sequence",seq).put("highlightTier",tier)
val own=raw("OWN_GOAL",3);val normal=raw("PASS_COMPLETED",1);val red=raw("RED_CARD",5)
check(LabV4PlaybackCoordinator.isHighlightEvent(own,LabViewingMode.SHORT_HIGHLIGHTS,false))
check(!LabV4PlaybackCoordinator.isHighlightEvent(normal,LabViewingMode.SHORT_HIGHLIGHTS,true))
check(!LabV4PlaybackCoordinator.isHighlightEvent(red,LabViewingMode.SHORT_HIGHLIGHTS,false))
check(LabV4PlaybackCoordinator.isHighlightEvent(red,LabViewingMode.SHORT_HIGHLIGHTS,true))
val rawEvents=JSONArray().put(normal).put(raw("SHOT",2)).put(own).put(raw("FULL_TIME",4))
val short=LabV4PlaybackCoordinator.filterEventsForViewingMode(rawEvents,LabViewingMode.SHORT_HIGHLIGHTS,false).map{it.getInt("sequence")}
check(short==listOf(2,3,4));println("PASS A1 production short-highlight own goal, lead-in and boundary filter")
val gate=PresentationSessionGate();val old=gate.begin("same-match");val fresh=gate.begin("same-match");check(!gate.isCurrent(old)&&gate.isCurrent(fresh));val other=gate.begin("other-match");check(!gate.isCurrent(fresh)&&gate.isCurrent(other));gate.invalidate();check(!gate.isCurrent(other));println("PASS A6 generation rejects stale same-match and cross-match callbacks")
val adapter=MatchEventCueAdapter()
val unseenGoal=raw("GOAL",1).put("side","HOME").put("actor","h").put("gameSecond",2701).put("period",1).put("periodElapsedSecond",2701)
val selectedOwn=raw("OWN_GOAL",2).put("side","AWAY").put("actor","h").put("gameSecond",2702).put("period",1).put("periodElapsedSecond",2702)
val rendered=adapter.convertV4StepEvents(JSONObject().put("homeGoals",1).put("awayGoals",1).put("gameSecond",2702),listOf(unseenGoal,selectedOwn),listOf(selectedOwn),LabViewingMode.SHORT_HIGHLIGHTS,{it.orEmpty()},{_,kind,_,_,_,_,h,a->"$kind:$h-$a"})
check(rendered.size==1&&rendered.single().sequence==2&&rendered.single().homeGoals==1&&rendered.single().awayGoals==1)
check(rendered.single().displayMinute=="45+1"&&rendered.single().text=="OWN_GOAL:1-1")
check(adapter.liveGameSecond==2702)
println("PASS A6 event-to-cue projection preserves filtered score, own goal and added time")
val tts=MatchTtsManager();val ns=NarrationScheduler(ctx,tts);ns.ingestSimulationEvents(listOf(event(1,1000),event(2,5000)));val ids=ns.exportLockedCueIds();check(ids.size==2);tts.ready(ids[1]);ns.onClockTick(5000);check(tts.played.isEmpty());tts.ready(ids[0]);check(tts.played.map{it.first}==listOf(ids[0]));println("PASS QA-06 production scheduler FIFO")
val before=ns.getLockedCuesSnapshot().map{it.cueId to it.text};ns.setPlaybackRate(2f);ns.setPlaybackRate(3f);check(ns.getLockedCuesSnapshot().map{it.cueId to it.text}==before);check(tts.rate==3f);tts.complete();check(tts.played.last().first==ids[1]&&tts.played.last().second==3f);println("PASS QA-07 production cue identity and audio rate")
tts.offset=400;val cp=ns.exportCheckpointCueInfo();val pending=ns.exportPendingCuesJson();val t2=MatchTtsManager();val restored=NarrationScheduler(ctx,t2);restored.pause();restored.restorePendingCuesJson(pending,cp.inProgressCueId,cp.inProgressCueOffsetMs);t2.ready(ids[1]);check(t2.played.isEmpty());restored.resume();restored.onClockTick(6000);check(t2.played.single().third==400L);println("PASS QA-09 production scheduler restores audio offset")
fun controller(stage:String,breakExpected:Boolean){TestClock.reset();var breaks=0;var ends=0;var sent=false;val local=NarrationScheduler(ctx,null);val session=LabRustLiveSession{target->if(!sent&&target>=30_000){sent=true;JSONObject().put("ok",true).put("simTimeMs",30_000L).put("stage",stage).put("isFullTime",!breakExpected).put("period",1).put("newEvents",JSONArray().put(JSONObject().put("sequence",1).put("simTimeMs",25_000L).put("kind","GOAL").put("side","HOME")))}else JSONObject().put("ok",true).put("simTimeMs",0L).put("stage","IN_HALF")};val ctrl=MatchPlaybackController("m",1,session,local,null,listener=object:MatchPlaybackController.Listener{override fun onEventsProduced(stepObj:JSONObject,rawEvents:List<JSONObject>,filteredEvents:List<JSONObject>,isLookahead:Boolean)=rawEvents.map{event(it.optInt("sequence"),it.optLong("simTimeMs"))};override fun onClockUpdated(snapshot:DesClockSnapshot,playbackRate:Float,paused:Boolean){};override fun onRosterUpdated(homeRoster:JSONObject){};override fun onPeriodBreak(stage:DesHalfStage,period:Int,homeGoals:Int,awayGoals:Int){breaks++};override fun onFastForwardSkip(fromSimTimeMs:Long,toSimTimeMs:Long){};override fun onMatchFinished(finalResult:JSONObject){ends++};override fun onError(error:Throwable){throw error}},workerExecutor=InlineExecutor());ctrl.start();TestClock.advance(1000);check(session.targets.any{it>=30_000});check(ctrl.presentedScore()==(0 to 0));check(breaks==0&&ends==0);val cues=local.exportLockedCueIds();ctrl.pause();val pausedTargets=session.targets.toList();TestClock.advance(11_000);check(session.targets.toList()==pausedTargets);check(ctrl.presentedScore()==(0 to 0));check(breaks==0&&ends==0);check(local.exportLockedCueIds()==cues);ctrl.submitCommandJson("{}"){ok,_->check(ok)};TestClock.advance(11_100);check(local.exportLockedCueIds()==cues);ctrl.resume();TestClock.advance(39_000);check(ctrl.presentedScore()==(1 to 0));check(breaks==0&&ends==0);TestClock.advance(42_000);check(if(breakExpected)breaks==1&&ends==0 else ends==1&&breaks==0);ctrl.release()}
controller("HALF_TIME_BREAK",true);println("PASS QA-03 production controller halftime, long event gap and score presentation");controller("FULL_TIME",false);println("PASS QA-04/05 production controller full time and confirmed narration on command");println("PASS: 7 QA scenarios against production Kotlin (fake clock, native session, JSON and audio)")
}finally{dir.deleteRecursively()}}
