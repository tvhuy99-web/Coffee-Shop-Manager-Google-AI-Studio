package pl.mkrstudio.truefootball3.audio
import java.io.File
import pl.mkrstudio.truefootball3.lab.TtsHealthStatus
class TtsSettingsStore(context: android.content.Context){data class Settings(val ratePercent:Int=100,val enabled:Boolean=true);fun reloadSettings()=Settings();companion object{const val DEFAULT_VI_SYLLABLES_PER_SEC=4.2f;fun estimateVietnameseSpeechDurationMs(text:String,rate:Float)=1000L}}
