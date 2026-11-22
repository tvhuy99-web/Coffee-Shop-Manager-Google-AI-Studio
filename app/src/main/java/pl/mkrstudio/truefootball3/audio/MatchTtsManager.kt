package pl.mkrstudio.truefootball3.audio

import android.content.Context
import android.media.AudioAttributes
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.speech.tts.TextToSpeech
import android.speech.tts.UtteranceProgressListener
import java.util.Locale
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicLong
import pl.mkrstudio.truefootball3.lab.LabLogStore

class MatchTtsManager(context: Context) {
    private val appContext: Context = context.applicationContext
    private val mainHandler = Handler(Looper.getMainLooper())
    private val selection: TtsSettingsStore.Selection = TtsSettingsStore(appContext).load()

    private var tts: TextToSpeech? = null
    private var pendingText: String? = null
    private var pendingCallback: Runnable? = null
    private var activeCallback: Runnable? = null
    private var timeout: Runnable? = null
    @Volatile private var activeUtteranceId: String? = null
    private var ready = false
    private var released = false
    private var initGeneration = 0

    @Volatile private var speechUnavailable = false
    private val consecutiveFailures = AtomicInteger()
    private val ttsRequested = AtomicInteger()
    private val ttsStarted = AtomicInteger()
    private val ttsDone = AtomicInteger()
    private val ttsError = AtomicInteger()
    private val ttsTimedOut = AtomicInteger()
    private val ttsStartTimes = ConcurrentHashMap<String, Long>()
    private val ttsAudibleMs = AtomicLong()

    fun getTtsMetricsJson(): String =
        "{\"speakRequests\":${ttsRequested.get()}" +
            ",\"ttsOnStart\":${ttsStarted.get()}" +
            ",\"ttsOnDone\":${ttsDone.get()}" +
            ",\"ttsOnError\":${ttsError.get()}" +
            ",\"ttsTimeout\":${ttsTimedOut.get()}" +
            ",\"approxSpeakingMs\":${ttsAudibleMs.get()}}"

    init {
        LabLogStore.log(
            "MATCH_TTS",
            "settings enabled=${selection.enabled}" +
                " engine=${printable(selection.enginePackage)}" +
                " language=${printable(selection.languageTag)}" +
                " voice=${printable(selection.voiceName)}" +
                " volume=${selection.volumePercent}" +
                " rate=${selection.ratePercent}" +
                " pitch=${selection.pitchPercent}"
        )
        if (selection.enabled) initializeEngine(selection.enginePackage, false)
    }

    private fun initializeEngine(packageName: String?, defaultFallback: Boolean) {
        if (released) return
        ready = false
        cancelTimeout()
        val generation = ++initGeneration
        var holder: TextToSpeech? = null
        val listener = TextToSpeech.OnInitListener { status ->
            mainHandler.post {
                val engine = holder
                if (released || generation != initGeneration || engine == null || engine !== tts) return@post
                cancelTimeout()
                LabLogStore.log(
                    "MATCH_TTS",
                    "onInit status=$status requestedEngine=${printable(packageName)} defaultFallback=$defaultFallback"
                )
                val configured = status == TextToSpeech.SUCCESS && applySelection(engine)
                if (configured) {
                    ready = true
                    speechUnavailable = false
                    consecutiveFailures.set(0)
                    LabLogStore.log("MATCH_TTS", "READY engine=${actualEngine(engine)}")
                    LabLogStore.log("TTS", "READY engine=${actualEngine(engine)} lang=${selection.languageTag} voice=${selection.voiceName}")
                    flushPending()
                } else {
                    ready = false
                    handleInitFailure(packageName, defaultFallback, "TTS initialization/configuration failed")
                }
            }
        }
        try {
            LabLogStore.log(
                "MATCH_TTS",
                "initialize requestedEngine=${printable(packageName)} defaultFallback=$defaultFallback"
            )
            holder = if (packageName.isNullOrEmpty()) {
                TextToSpeech(appContext, listener)
            } else {
                TextToSpeech(appContext, listener, packageName)
            }
            tts = holder
            timeout = Runnable {
                if (released || generation != initGeneration || ready) return@Runnable
                LabLogStore.log("MATCH_TTS", "initialization timeout requestedEngine=${printable(packageName)}")
                handleInitFailure(packageName, defaultFallback, "TTS initialization timeout")
            }.also { mainHandler.postDelayed(it, 7000L) }
        } catch (error: RuntimeException) {
            tts = null
            LabLogStore.error("MATCH_TTS", "TextToSpeech constructor failed engine=${printable(packageName)}", error)
            handleInitFailure(packageName, defaultFallback, "TTS constructor failed")
        }
    }

    private fun handleInitFailure(packageName: String?, defaultFallback: Boolean, reason: String) {
        if (released) return
        cancelTimeout()
        val failed = tts
        tts = null
        if (failed != null) {
            try {
                failed.shutdown()
            } catch (_: RuntimeException) {
            }
        }
        if (!defaultFallback && !packageName.isNullOrEmpty()) {
            LabLogStore.log("MATCH_TTS", "$reason; retrying with system default engine")
            initializeEngine("", true)
            return
        }
        LabLogStore.log("MATCH_TTS", "$reason; TTS unavailable for this match")
        speechUnavailable = true
        finishPending()
    }

    private fun applySelection(engine: TextToSpeech): Boolean {
        return try {
            engine.setAudioAttributes(
                AudioAttributes.Builder()
                    .setUsage(AudioAttributes.USAGE_MEDIA)
                    .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
                    .build()
            )
            val rateResult = engine.setSpeechRate(selection.ratePercent / 100f)
            if (rateResult == TextToSpeech.ERROR) {
                LabLogStore.log("MATCH_TTS", "engine rejected speech rate; continuing with engine default")
            }
            val pitchResult = engine.setPitch(selection.pitchPercent / 100f)
            if (pitchResult == TextToSpeech.ERROR) {
                LabLogStore.log("MATCH_TTS", "engine rejected pitch; continuing with engine default")
            }

            val requested = Locale.forLanguageTag(selection.languageTag)
            var languageResult = engine.setLanguage(requested)
            LabLogStore.log("MATCH_TTS", "setLanguage requested=${requested.toLanguageTag()} result=$languageResult")
            if (languageResult < TextToSpeech.LANG_AVAILABLE && requested.language.isNotEmpty()) {
                val languageOnly = Locale(requested.language)
                languageResult = engine.setLanguage(languageOnly)
                LabLogStore.log(
                    "MATCH_TTS",
                    "full locale unavailable; language-only=${languageOnly.toLanguageTag()} result=$languageResult"
                )
            }
            if (languageResult < TextToSpeech.LANG_AVAILABLE) {
                val deviceLocale = Locale.getDefault()
                languageResult = engine.setLanguage(deviceLocale)
                LabLogStore.log(
                    "MATCH_TTS",
                    "requested language unavailable; device locale=${deviceLocale.toLanguageTag()} result=$languageResult"
                )
            }
            if (languageResult < TextToSpeech.LANG_AVAILABLE) {
                LabLogStore.log(
                    "MATCH_TTS",
                    "engine rejected all requested locales; using engine default voice/language"
                )
            }

            if (selection.voiceName.isNotEmpty()) {
                var found = false
                val voices = engine.voices
                if (voices != null) {
                    for (voice in voices) {
                        if (selection.voiceName == voice.name) {
                            val result = engine.setVoice(voice)
                            found = result != TextToSpeech.ERROR
                            LabLogStore.log("MATCH_TTS", "setVoice name=${selection.voiceName} result=$result")
                            break
                        }
                    }
                }
                if (!found) {
                    LabLogStore.log(
                        "MATCH_TTS",
                        "saved voice not available; using engine default voice: ${selection.voiceName}"
                    )
                }
            }
            true
        } catch (error: RuntimeException) {
            LabLogStore.error("MATCH_TTS", "applySelection failed", error)
            false
        }
    }

    fun isEnabled(): Boolean = selection.enabled

    @JvmOverloads
    fun speak(text: String?, onComplete: Runnable? = null) {
        if (released || speechUnavailable || !selection.enabled || text.isNullOrBlank()) {
            LabLogStore.log(
                "MATCH_TTS",
                "speak skipped released=$released enabled=${selection.enabled}" +
                    " unavailable=$speechUnavailable empty=${text.isNullOrBlank()}"
            )
            complete(onComplete)
            return
        }
        val value = text.trim()
        if (!ready || tts == null) {
            LabLogStore.log("MATCH_TTS", "speak queued while engine initializes chars=${value.length}")
            complete(pendingCallback)
            pendingText = value
            pendingCallback = onComplete
            return
        }
        speakNow(value, onComplete)
    }

    private fun flushPending() {
        val text = pendingText ?: return
        if (!ready) return
        val callback = pendingCallback
        pendingText = null
        pendingCallback = null
        speakNow(text, callback)
    }

    private fun speakNow(text: String, onComplete: Runnable?) {
        finishActive()
        val engine = tts ?: run {
            complete(onComplete)
            return
        }
        val parameters = Bundle().apply {
            putFloat(TextToSpeech.Engine.KEY_PARAM_VOLUME, selection.volumePercent / 100f)
        }
        val utteranceId = "match_commentary_${System.nanoTime()}"
        activeUtteranceId = utteranceId
        activeCallback = onComplete
        ttsRequested.incrementAndGet()
        engine.setOnUtteranceProgressListener(object : UtteranceProgressListener() {
            override fun onStart(id: String) {
                if (id != activeUtteranceId) return
                ttsStarted.incrementAndGet()
                ttsStartTimes[id] = SystemClock.elapsedRealtime()
                LabLogStore.log("MATCH_TTS", "utterance START id=$id")
            }

            override fun onDone(id: String) {
                if (id != activeUtteranceId) return
                ttsDone.incrementAndGet()
                consecutiveFailures.set(0)
                val started = ttsStartTimes.remove(id)
                if (started != null) {
                    ttsAudibleMs.addAndGet(maxOf(0L, SystemClock.elapsedRealtime() - started))
                }
                LabLogStore.log("MATCH_TTS", "utterance DONE id=$id")
                finishUtterance(id)
            }

            @Deprecated("Deprecated in Java")
            override fun onError(id: String) {
                if (id != activeUtteranceId) return
                ttsStartTimes.remove(id)
                ttsError.incrementAndGet()
                recordSpeechFailure("onError")
                LabLogStore.log("MATCH_TTS", "utterance ERROR id=$id")
                finishUtterance(id)
            }

            override fun onError(id: String, errorCode: Int) {
                if (id != activeUtteranceId) return
                ttsStartTimes.remove(id)
                ttsError.incrementAndGet()
                recordSpeechFailure("onError code=$errorCode")
                LabLogStore.log("MATCH_TTS", "utterance ERROR id=$id code=$errorCode")
                finishUtterance(id)
            }
        })
        try {
            val result = engine.speak(text, TextToSpeech.QUEUE_FLUSH, parameters, utteranceId)
            LabLogStore.log(
                "MATCH_TTS",
                "speak result=$result chars=${text.length} volume=${selection.volumePercent} id=$utteranceId"
            )
            if (result == TextToSpeech.ERROR) {
                ttsError.incrementAndGet()
                recordSpeechFailure("speak returned error")
                finishUtterance(utteranceId)
                return
            }
            val estimatedWait = 1200L + Math.round((text.length * 90L) * (100f / maxOf(50, selection.ratePercent)))
            val maximumWait = estimatedWait.coerceIn(2500L, 30000L)
            timeout = Runnable {
                ttsTimedOut.incrementAndGet()
                recordSpeechFailure("utterance timeout")
                LabLogStore.log("MATCH_TTS", "utterance timeout id=$utteranceId")
                finishUtterance(utteranceId)
            }.also { mainHandler.postDelayed(it, maximumWait) }
        } catch (error: RuntimeException) {
            ttsError.incrementAndGet()
            recordSpeechFailure("speak threw")
            LabLogStore.error("MATCH_TTS", "speak threw exception id=$utteranceId", error)
            finishUtterance(utteranceId)
        }
    }

    private fun recordSpeechFailure(reason: String) {
        val consecutive = consecutiveFailures.incrementAndGet()
        if (consecutive >= 3 && !speechUnavailable) {
            speechUnavailable = true
            LabLogStore.log(
                "MATCH_TTS",
                "TTS circuit breaker after $consecutive consecutive failures ($reason); continuing match in text mode"
            )
        }
    }

    private fun finishUtterance(id: String?) {
        mainHandler.post {
            if (id == null || id != activeUtteranceId) return@post
            finishActive()
        }
    }

    private fun finishActive() {
        cancelTimeout()
        activeUtteranceId?.let { ttsStartTimes.remove(it) }
        activeUtteranceId = null
        val callback = activeCallback
        activeCallback = null
        complete(callback)
    }

    private fun finishPending() {
        pendingText = null
        val callback = pendingCallback
        pendingCallback = null
        complete(callback)
    }

    private fun cancelTimeout() {
        timeout?.let { mainHandler.removeCallbacks(it) }
        timeout = null
    }

    private fun complete(callback: Runnable?) {
        if (callback != null) mainHandler.post(callback)
    }

    fun stop() {
        LabLogStore.log("MATCH_TTS", "stop")
        finishPending()
        tts?.let {
            try {
                it.stop()
            } catch (_: RuntimeException) {
            }
        }
        finishActive()
    }

    fun release() {
        if (released) return
        LabLogStore.log("MATCH_TTS", "release")
        released = true
        ready = false
        initGeneration++
        finishPending()
        finishActive()
        tts?.let {
            try {
                it.stop()
            } catch (_: RuntimeException) {
            }
            try {
                it.shutdown()
            } catch (_: RuntimeException) {
            }
            tts = null
        }
        mainHandler.removeCallbacksAndMessages(null)
    }

    companion object {
        private fun actualEngine(engine: TextToSpeech?): String {
            if (engine == null) return "null"
            return try {
                printable(engine.defaultEngine)
            } catch (_: RuntimeException) {
                "unknown"
            }
        }

        private fun printable(value: String?): String =
            if (value.isNullOrBlank()) "<default>" else value.trim()
    }
}
