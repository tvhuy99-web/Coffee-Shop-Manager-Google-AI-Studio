package pl.mkrstudio.truefootball3.lab

import android.content.Context
import android.content.SharedPreferences

/**
 * Independent storage for the "Cài đặt mô phỏng" menu (separate from TTS and Sound settings).
 * Synchronizes viewing mode (Full trận / Chi tiết / Ngắn gọn), simulation engine mode (3.0 / 4.0),
 * and TTS-driven match pacing between Settings and the Trial match screen.
 */
class SimulationSettingsStore(context: Context) {
    data class Settings(
        val viewingMode: LabViewingMode = LabViewingMode.FULL_MATCH,
        val engineVersion: LabEngineVersion = LabEngineVersion.V4,
        val includeTurningPointsInShort: Boolean = true,
        val syncMatchSpeedWithTts: Boolean = true
    )

    private val preferences: SharedPreferences =
        context.applicationContext.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    fun load(): Settings {
        val rawMode = preferences.getString(VIEWING_MODE, LabViewingMode.FULL_MATCH.name)
        val mode = try {
            LabViewingMode.valueOf(rawMode ?: LabViewingMode.FULL_MATCH.name)
        } catch (_: IllegalArgumentException) {
            LabViewingMode.FULL_MATCH
        }
        val rawEngine = preferences.getString(ENGINE_VERSION, LabEngineVersion.V4.name)
        val engine = try {
            LabEngineVersion.valueOf(rawEngine ?: LabEngineVersion.V4.name)
        } catch (_: IllegalArgumentException) {
            LabEngineVersion.V4
        }
        return Settings(
            viewingMode = mode,
            engineVersion = engine,
            includeTurningPointsInShort = preferences.getBoolean(INCLUDE_TURNING_POINTS, true),
            syncMatchSpeedWithTts = preferences.getBoolean(SYNC_SPEED_WITH_TTS, true)
        )
    }

    fun save(settings: Settings) {
        preferences.edit()
            .putString(VIEWING_MODE, settings.viewingMode.name)
            .putString(ENGINE_VERSION, settings.engineVersion.name)
            .putBoolean(INCLUDE_TURNING_POINTS, settings.includeTurningPointsInShort)
            .putBoolean(SYNC_SPEED_WITH_TTS, settings.syncMatchSpeedWithTts)
            .apply()
    }

    fun updateViewingMode(mode: LabViewingMode) {
        preferences.edit().putString(VIEWING_MODE, mode.name).apply()
    }

    fun updateEngineVersion(version: LabEngineVersion) {
        preferences.edit().putString(ENGINE_VERSION, version.name).apply()
    }

    companion object {
        private const val PREFS = "tf3SimulationSettings"
        private const val VIEWING_MODE = "viewingMode"
        private const val ENGINE_VERSION = "engineVersion"
        private const val INCLUDE_TURNING_POINTS = "includeTurningPointsInShort"
        private const val SYNC_SPEED_WITH_TTS = "syncMatchSpeedWithTts"
    }
}
