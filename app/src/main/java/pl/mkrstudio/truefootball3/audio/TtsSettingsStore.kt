package pl.mkrstudio.truefootball3.audio

import android.content.Context
import android.content.SharedPreferences
import java.util.Locale

class TtsSettingsStore(context: Context) {
    class Selection(
        @JvmField val enabled: Boolean,
        enginePackage: String?,
        languageTag: String?,
        voiceName: String?,
        volumePercent: Int,
        ratePercent: Int,
        pitchPercent: Int
    ) {
        @JvmField val enginePackage: String = clean(enginePackage)
        @JvmField val languageTag: String = normalizeLanguageTag(languageTag)
        @JvmField val voiceName: String = clean(voiceName)
        @JvmField val volumePercent: Int = volumePercent.coerceIn(0, 100)
        @JvmField val ratePercent: Int = ratePercent.coerceIn(50, 500)
        @JvmField val pitchPercent: Int = pitchPercent.coerceIn(50, 200)
    }

    private val preferences: SharedPreferences =
        context.applicationContext.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    fun load(): Selection = Selection(
        preferences.getBoolean(ENABLED, true),
        preferences.getString(ENGINE, ""),
        preferences.getString(LANGUAGE, DEFAULT_LANGUAGE),
        preferences.getString(VOICE, ""),
        preferences.getInt(VOLUME, 100),
        preferences.getInt(RATE, 100),
        preferences.getInt(PITCH, 100)
    )

    fun save(selection: Selection?) {
        val safe = selection ?: Selection(true, "", DEFAULT_LANGUAGE, "", 100, 100, 100)
        preferences.edit()
            .putBoolean(ENABLED, safe.enabled)
            .putString(ENGINE, safe.enginePackage)
            .putString(LANGUAGE, safe.languageTag)
            .putString(VOICE, safe.voiceName)
            .putInt(VOLUME, safe.volumePercent)
            .putInt(RATE, safe.ratePercent)
            .putInt(PITCH, safe.pitchPercent)
            .apply()
    }

    companion object {
        const val DEFAULT_LANGUAGE = "vi-VN"
        private const val PREFS = "tf3TtsSettings"
        private const val ENABLED = "enabled"
        private const val ENGINE = "enginePackage"
        private const val LANGUAGE = "languageTag"
        private const val VOICE = "voiceName"
        private const val VOLUME = "volumePercent"
        private const val RATE = "ratePercent"
        private const val PITCH = "pitchPercent"

        @JvmStatic
        fun normalizeLanguageTag(raw: String?): String {
            val value = clean(raw).replace('_', '-').ifEmpty { DEFAULT_LANGUAGE }
            val locale = Locale.forLanguageTag(value)
            if (locale.language.isNullOrEmpty() || locale.language == "und") {
                return DEFAULT_LANGUAGE
            }
            val tag = locale.toLanguageTag()
            return tag.ifEmpty { DEFAULT_LANGUAGE }
        }

        private fun clean(value: String?): String = value?.trim().orEmpty()
    }
}
