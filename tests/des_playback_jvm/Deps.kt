package pl.mkrstudio.truefootball3.lab
import org.json.*
object LabLogStore{fun log(k:String,m:String){};fun error(k:String,m:String,e:Throwable){throw AssertionError(m,e)}}
// Only offline sequence playback touches this settings double; the filter is real production code.
class SimulationSettingsStore(context: android.content.Context) {
    data class Settings(val syncMatchSpeedWithTts:Boolean=true)
    fun load()=Settings()
}
