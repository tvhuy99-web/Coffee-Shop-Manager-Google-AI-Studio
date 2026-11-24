package pl.mkrstudio.truefootball3.audio

import android.content.Context
import android.content.SharedPreferences
import java.util.Locale
import kotlin.math.roundToLong

class TtsSettingsStore(context: Context) {
    class Selection @JvmOverloads constructor(
        @JvmField val enabled: Boolean,
        enginePackage: String?,
        languageTag: String?,
        voiceName: String?,
        volumePercent: Int,
        ratePercent: Int,
        pitchPercent: Int,
        targetSyllablesPerSec: Float = DEFAULT_VI_SYLLABLES_PER_SEC
    ) {
        @JvmField val enginePackage: String = clean(enginePackage)
        @JvmField val languageTag: String = normalizeLanguageTag(languageTag)
        @JvmField val voiceName: String = clean(voiceName)
        @JvmField val volumePercent: Int = volumePercent.coerceIn(0, 100)
        @JvmField val ratePercent: Int = ratePercent.coerceIn(50, 500)
        @JvmField val pitchPercent: Int = pitchPercent.coerceIn(50, 200)
        @JvmField val targetSyllablesPerSec: Float = targetSyllablesPerSec.coerceIn(2.0f, 8.0f)
    }

    private val preferences: SharedPreferences =
        context.applicationContext.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    @Volatile
    private var cachedSelection: Selection? = null

    fun load(): Selection {
        val loaded = Selection(
            preferences.getBoolean(ENABLED, true),
            preferences.getString(ENGINE, ""),
            preferences.getString(LANGUAGE, DEFAULT_LANGUAGE),
            preferences.getString(VOICE, ""),
            preferences.getInt(VOLUME, 100),
            preferences.getInt(RATE, 100),
            preferences.getInt(PITCH, 100),
            preferences.getFloat(VI_SYLLABLES_PER_SEC, DEFAULT_VI_SYLLABLES_PER_SEC)
        )
        cachedSelection = loaded
        return loaded
    }

    /** Dynamically reloads the latest persisted TTS configuration from SharedPreferences. */
    fun reloadSettings(): Selection = load()

    fun current(): Selection = cachedSelection ?: load()

    fun save(selection: Selection?) {
        val safe = selection ?: Selection(
            true,
            "",
            DEFAULT_LANGUAGE,
            "",
            100,
            100,
            100,
            DEFAULT_VI_SYLLABLES_PER_SEC
        )
        cachedSelection = safe
        preferences.edit()
            .putBoolean(ENABLED, safe.enabled)
            .putString(ENGINE, safe.enginePackage)
            .putString(LANGUAGE, safe.languageTag)
            .putString(VOICE, safe.voiceName)
            .putInt(VOLUME, safe.volumePercent)
            .putInt(RATE, safe.ratePercent)
            .putInt(PITCH, safe.pitchPercent)
            .putFloat(VI_SYLLABLES_PER_SEC, safe.targetSyllablesPerSec)
            .apply()
    }


    fun saveCalibratedBaseSpeechRate(voiceKey: String, speechRate: Float, measuredSyllablesPerSec: Float) {
        val safeKey = voiceKey.ifBlank { "default" }
        preferences.edit()
            .putFloat("cal_rate_$safeKey", speechRate.coerceIn(0.65f, 1.65f))
            .putFloat("cal_syl_$safeKey", measuredSyllablesPerSec.coerceIn(1.5f, 10.0f))
            .apply()
    }

    fun loadCalibratedBaseSpeechRate(voiceKey: String): Float? {
        val safeKey = voiceKey.ifBlank { "default" }
        val key = "cal_rate_$safeKey"
        return if (preferences.contains(key)) preferences.getFloat(key, 1.0f).coerceIn(0.65f, 1.65f) else null
    }

    companion object {
        const val CALIBRATION_PROBE_TEXT_VI = "Trọng tài chính vừa thổi còi bắt đầu hiệp một của trận đấu bóng đá hôm nay tại sân vận động"
        const val CALIBRATION_PROBE_SYLLABLES = 21

        const val DEFAULT_LANGUAGE = "vi-VN"
        const val DEFAULT_VI_SYLLABLES_PER_SEC = 4.2f
        private const val PREFS = "tf3TtsSettings"
        private const val ENABLED = "enabled"
        private const val ENGINE = "enginePackage"
        private const val LANGUAGE = "languageTag"
        private const val VOICE = "voiceName"
        private const val VOLUME = "volumePercent"
        private const val RATE = "ratePercent"
        private const val PITCH = "pitchPercent"
        private const val VI_SYLLABLES_PER_SEC = "viSyllablesPerSec"

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

        /**
         * Estimates baseline 1.0x Vietnamese speech duration in milliseconds from syllable tokens
         * using `DEFAULT_VI_SYLLABLES_PER_SEC` (4.2 syllables/sec) as the initial fallback before
         * actual WAV header measurement is available.
         */
        @JvmStatic
        fun estimateVietnameseSpeechDurationMs(
            text: String,
            syllablesPerSec: Float = DEFAULT_VI_SYLLABLES_PER_SEC
        ): Long {
            val trimmed = text.trim()
            if (trimmed.isEmpty()) return 0L
            val tokens = trimmed.split(Regex("\\s+")).count { it.isNotBlank() }.coerceAtLeast(1)
            val rate = syllablesPerSec.coerceIn(2.0f, 8.0f)
            val punctuationPauseMs = trimmed.count { it == ',' || it == ';' } * 120L +
                trimmed.count { it == '.' || it == '!' || it == '?' } * 220L
            val speechMs = ((tokens / rate) * 1000f).roundToLong()
            return (speechMs + punctuationPauseMs).coerceIn(500L, 18_000L)
        }

        private fun clean(value: String?): String = value?.trim().orEmpty()
    }
}
