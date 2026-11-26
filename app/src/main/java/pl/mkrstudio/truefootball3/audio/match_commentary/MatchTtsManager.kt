package pl.mkrstudio.truefootball3.match_commentary

import pl.mkrstudio.truefootball3.audio.*

import android.content.Context
import android.media.AudioAttributes
import android.media.MediaMetadataRetriever
import android.media.MediaPlayer
import android.media.PlaybackParams
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.speech.tts.TextToSpeech
import android.speech.tts.UtteranceProgressListener
import java.io.File
import java.io.RandomAccessFile
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.ArrayDeque
import java.util.Locale
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicLong
import pl.mkrstudio.truefootball3.lab.LabLogStore
import pl.mkrstudio.truefootball3.lab.TtsHealthStatus

/**
 * DES v1.4 TTS & pitch-preserving WAV playback manager.
 *
 * Key upgrades over legacy V3/V4 TTS:
 * - Dynamic configuration reload via `reloadSettings()`.
 * - Offline cue synthesis via `synthesizeCueToWav(...)` + exact RIFF/WAVE duration parsing
 *   (44-byte header `dataSize / byteRate * 1000` with chunk scan and `MediaMetadataRetriever` fallback).
 * - Pitch-preserving playback via `android.media.MediaPlayer` + `PlaybackParams().setSpeed(rate).setPitch(1.0f)`
 *   supporting `1.0x / 1.25x / 1.5x / 2.0x / 3.0x` smoothly without external `androidx.media3`.
 * - Removed silent circuit breaker (`consecutive >= 3`); transparently reports `TtsHealthStatus`
 *   (`HEALTHY`, `DEGRADED_BACKLOG`, `BACKEND_ERROR`, `UNAVAILABLE`) via listener without muting TTS.
 */
class MatchTtsManager(context: Context) {
    private data class SynthRequest(
        val cueId: String,
        val utteranceId: String,
        val text: String,
        val outFile: File,
        val onReady: (durationMs: Long) -> Unit,
        val onError: (String) -> Unit,
        var timeoutRunnable: Runnable? = null
    )

    private val appContext: Context = context.applicationContext
    private val settingsStore = TtsSettingsStore(appContext)
    private val mainHandler = Handler(Looper.getMainLooper())
    @Volatile private var selection: TtsSettingsStore.Selection = settingsStore.load()
    @Volatile private var calibratedBaseSpeechRate: Float = 1.0f
    @Volatile private var calibrationInProgress: Boolean = false

    private var tts: TextToSpeech? = null
    private var pendingText: String? = null
    private var pendingCallback: Runnable? = null
    private var activeCallback: Runnable? = null
    private var timeout: Runnable? = null
    @Volatile private var activeUtteranceId: String? = null
    @Volatile private var ready = false
    @Volatile private var released = false
    private var initGeneration = 0

    @Volatile private var speechUnavailable = false
    @Volatile private var healthStatus: TtsHealthStatus = TtsHealthStatus.HEALTHY
    @Volatile private var healthListener: ((TtsHealthStatus, String) -> Unit)? = null

    private val pendingSynthQueue = ArrayDeque<SynthRequest>()
    private val activeSynthRequests = ConcurrentHashMap<String, SynthRequest>()

    private val wavPlayerLock = Any()
    private var wavPlayer: MediaPlayer? = null
    @Volatile private var wavPlaying = false
    @Volatile private var wavPaused = false
    @Volatile private var activePlaybackRate = 1.0f

    private val ttsRequested = AtomicInteger()
    private val ttsStarted = AtomicInteger()
    private val ttsDone = AtomicInteger()
    private val ttsError = AtomicInteger()
    private val ttsTimedOut = AtomicInteger()
    private val synthRequested = AtomicInteger()
    private val synthDone = AtomicInteger()
    private val synthError = AtomicInteger()
    private val ttsStartTimes = ConcurrentHashMap<String, Long>()
    private val ttsAudibleMs = AtomicLong()

    fun getTtsMetricsJson(): String =
        "{\"speakRequests\":${ttsRequested.get()}" +
            ",\"ttsOnStart\":${ttsStarted.get()}" +
            ",\"ttsOnDone\":${ttsDone.get()}" +
            ",\"ttsOnError\":${ttsError.get()}" +
            ",\"ttsTimeout\":${ttsTimedOut.get()}" +
            ",\"synthRequests\":${synthRequested.get()}" +
            ",\"synthDone\":${synthDone.get()}" +
            ",\"synthError\":${synthError.get()}" +
            ",\"health\":\"${healthStatus.name}\"" +
            ",\"approxSpeakingMs\":${ttsAudibleMs.get()}}"

    init {
        logSelection(selection)
        if (selection.enabled) {
            initializeEngine(selection.enginePackage, false)
        } else {
            updateHealth(TtsHealthStatus.UNAVAILABLE, "TTS disabled in settings")
        }
    }

    fun setHealthListener(listener: ((TtsHealthStatus, String) -> Unit)?) {
        healthListener = listener
        listener?.invoke(healthStatus, "initial")
    }

    fun currentHealthStatus(): TtsHealthStatus = healthStatus

    fun reportBacklogDegraded(reason: String) {
        if (healthStatus != TtsHealthStatus.UNAVAILABLE) {
            updateHealth(TtsHealthStatus.DEGRADED_BACKLOG, reason)
        }
    }

    fun reportHealthy(reason: String = "recovered") {
        if (!speechUnavailable && selection.enabled && ready) {
            updateHealth(TtsHealthStatus.HEALTHY, reason)
        }
    }

    fun currentSelection(): TtsSettingsStore.Selection = selection

    /**
     * Dynamically reloads settings from `TtsSettingsStore` and re-applies rate/pitch/language/voice
     * or re-initializes the engine if the selected engine package or enabled flag changed.
     */
    fun reloadSettings(): TtsSettingsStore.Selection {
        val previous = selection
        val updated = settingsStore.reloadSettings()
        selection = updated
        logSelection(updated)

        if (released) return updated

        if (!updated.enabled) {
            stop()
            stopWavPlayback()
            updateHealth(TtsHealthStatus.UNAVAILABLE, "TTS disabled via reloadSettings")
            return updated
        }

        val engineChanged = previous.enginePackage != updated.enginePackage
        val currentEngine = tts
        if (currentEngine == null || engineChanged) {
            speechUnavailable = false
            shutdownEngineOnly()
            initializeEngine(updated.enginePackage, false)
        } else if (ready) {
            val ok = applySelection(currentEngine)
            if (ok) {
                updateHealth(TtsHealthStatus.HEALTHY, "settings reloaded")
            } else {
                updateHealth(TtsHealthStatus.BACKEND_ERROR, "applySelection failed on reload")
            }
        }

        synchronized(wavPlayerLock) {
            val vol = updated.volumePercent / 100f
            try {
                wavPlayer?.setVolume(vol, vol)
            } catch (_: RuntimeException) {
            }
        }
        return updated
    }

    private fun logSelection(sel: TtsSettingsStore.Selection) {
        LabLogStore.log(
            "MATCH_TTS",
            "settings enabled=${sel.enabled}" +
                " engine=${printable(sel.enginePackage)}" +
                " language=${printable(sel.languageTag)}" +
                " voice=${printable(sel.voiceName)}" +
                " volume=${sel.volumePercent}" +
                " rate=${sel.ratePercent}" +
                " pitch=${sel.pitchPercent}" +
                " viSyllablesPerSec=${sel.targetSyllablesPerSec}"
        )
    }

    private fun updateHealth(newStatus: TtsHealthStatus, detail: String) {
        val changed = healthStatus != newStatus
        healthStatus = newStatus
        if (changed) {
            LabLogStore.log("MATCH_TTS", "TtsHealthStatus -> $newStatus ($detail)")
        }
        val listener = healthListener
        if (listener != null) {
            mainHandler.post { listener.invoke(newStatus, detail) }
        }
    }

    private fun shutdownEngineOnly() {
        ready = false
        cancelTimeout()
        val old = tts
        tts = null
        if (old != null) {
            try {
                old.stop()
            } catch (_: RuntimeException) {
            }
            try {
                old.shutdown()
            } catch (_: RuntimeException) {
            }
        }
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
                    updateHealth(TtsHealthStatus.HEALTHY, "engine ready (${actualEngine(engine)})")
                    LabLogStore.log("MATCH_TTS", "READY engine=${actualEngine(engine)}")
                    LabLogStore.log("TTS", "READY engine=${actualEngine(engine)} lang=${selection.languageTag} voice=${selection.voiceName}")
                    calibrateVietnameseBaseSpeechRate(appContext.cacheDir)
                    flushPending()
                    flushPendingSyntheses()
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
        updateHealth(TtsHealthStatus.UNAVAILABLE, reason)
        finishPending()
        failAllPendingSyntheses(reason)
    }

    private fun installUnifiedProgressListener(engine: TextToSpeech) {
        engine.setOnUtteranceProgressListener(object : UtteranceProgressListener() {
            override fun onStart(id: String) {
                if (id == activeUtteranceId) {
                    ttsStarted.incrementAndGet()
                    ttsStartTimes[id] = SystemClock.elapsedRealtime()
                    LabLogStore.log("MATCH_TTS", "utterance START id=$id")
                }
            }

            override fun onDone(id: String) {
                val synth = activeSynthRequests.remove(id)
                if (synth != null) {
                    synth.timeoutRunnable?.let { mainHandler.removeCallbacks(it) }
                    synthDone.incrementAndGet()
                    val durationMs = measureWavDurationMs(synth.outFile, synth.text, selection.targetSyllablesPerSec)
                    if (healthStatus == TtsHealthStatus.BACKEND_ERROR) {
                        updateHealth(TtsHealthStatus.HEALTHY, "synthesis recovered (${synth.cueId})")
                    }
                    LabLogStore.log(
                        "MATCH_TTS",
                        "synth DONE cueId=${synth.cueId} file=${synth.outFile.name} durationMs=$durationMs"
                    )
                    mainHandler.post { synth.onReady(durationMs) }
                    return
                }
                if (id != activeUtteranceId) return
                ttsDone.incrementAndGet()
                val started = ttsStartTimes.remove(id)
                if (started != null) {
                    ttsAudibleMs.addAndGet(maxOf(0L, SystemClock.elapsedRealtime() - started))
                }
                if (healthStatus == TtsHealthStatus.BACKEND_ERROR) {
                    updateHealth(TtsHealthStatus.HEALTHY, "speak recovered")
                }
                LabLogStore.log("MATCH_TTS", "utterance DONE id=$id")
                finishUtterance(id)
            }

            @Deprecated("Deprecated in Java")
            override fun onError(id: String) {
                handleUtteranceError(id, -1)
            }

            override fun onError(id: String, errorCode: Int) {
                handleUtteranceError(id, errorCode)
            }
        })
    }

    private fun handleUtteranceError(id: String, errorCode: Int) {
        val synth = activeSynthRequests.remove(id)
        if (synth != null) {
            synth.timeoutRunnable?.let { mainHandler.removeCallbacks(it) }
            synthError.incrementAndGet()
            val msg = "synthesizeToFile error code=$errorCode cueId=${synth.cueId}"
            LabLogStore.log("MATCH_TTS", msg)
            updateHealth(TtsHealthStatus.BACKEND_ERROR, msg)
            mainHandler.post { synth.onError(msg) }
            return
        }
        if (id != activeUtteranceId) return
        ttsStartTimes.remove(id)
        ttsError.incrementAndGet()
        val msg = "utterance error code=$errorCode id=$id"
        LabLogStore.log("MATCH_TTS", msg)
        updateHealth(TtsHealthStatus.BACKEND_ERROR, msg)
        finishUtterance(id)
    }

    private fun applySelection(engine: TextToSpeech): Boolean {
        return try {
            engine.setAudioAttributes(
                AudioAttributes.Builder()
                    .setUsage(AudioAttributes.USAGE_MEDIA)
                    .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
                    .build()
            )
            // Always synthesize baseline 1.0x cues at natural speech rate unless overridden in settings.
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
            installUnifiedProgressListener(engine)
            true
        } catch (error: RuntimeException) {
            LabLogStore.error("MATCH_TTS", "applySelection failed", error)
            false
        }
    }

    fun isEnabled(): Boolean = selection.enabled

    fun isReady(): Boolean = ready && !released && !speechUnavailable && selection.enabled

    /**
     * Synthesizes a commentary cue offline into a WAV file at canonical 1.0x cadence and reports
     * the measured WAV duration in milliseconds from the RIFF/WAVE header (with MediaMetadataRetriever fallback).
     */
    fun synthesizeCueToWav(
        cueId: String,
        text: String,
        outFile: File,
        onReady: (durationMs: Long) -> Unit,
        onError: (String) -> Unit
    ) {
        val trimmed = text.trim()
        if (released || !selection.enabled || speechUnavailable || trimmed.isEmpty()) {
            val reason = when {
                released -> "tts_released"
                !selection.enabled -> "tts_disabled"
                speechUnavailable -> "tts_unavailable"
                else -> "empty_text"
            }
            mainHandler.post { onError(reason) }
            return
        }

        val utteranceId = "des_cue_${cueId}_${System.nanoTime()}"
        val request = SynthRequest(
            cueId = cueId,
            utteranceId = utteranceId,
            text = trimmed,
            outFile = outFile,
            onReady = onReady,
            onError = onError
        )
        if (!ready || tts == null) {
            synchronized(pendingSynthQueue) {
                pendingSynthQueue.addLast(request)
            }
            return
        }
        dispatchSynthRequest(request)
    }

    private fun flushPendingSyntheses() {
        val batch = ArrayList<SynthRequest>()
        synchronized(pendingSynthQueue) {
            while (pendingSynthQueue.isNotEmpty()) {
                batch.add(pendingSynthQueue.removeFirst())
            }
        }
        for (req in batch) {
            dispatchSynthRequest(req)
        }
    }

    private fun failAllPendingSyntheses(reason: String) {
        val batch = ArrayList<SynthRequest>()
        synchronized(pendingSynthQueue) {
            while (pendingSynthQueue.isNotEmpty()) {
                batch.add(pendingSynthQueue.removeFirst())
            }
        }
        for (req in batch) {
            mainHandler.post { req.onError(reason) }
        }
    }

    /**
     * CRITICAL-05: Measures the active Android TTS voice's actual speaking speed on a known
     * 21-syllable Vietnamese probe sentence and computes `calibratedBaseSpeechRate` so that
     * game 1.0x targets `TtsSettingsStore.DEFAULT_VI_SYLLABLES_PER_SEC` (4.2 syl/s) regardless
     * of the device TTS engine's default speed.
     */
    fun calibrateVietnameseBaseSpeechRate(
        cacheDir: File = appContext.cacheDir,
        onCalibrated: ((calibratedRate: Float, measuredSyllablesPerSec: Float) -> Unit)? = null
    ) {
        val voiceKey = "${selection.enginePackage}|${selection.voiceName}|${selection.languageTag}"
        settingsStore.loadCalibratedBaseSpeechRate(voiceKey)?.let { cached ->
            calibratedBaseSpeechRate = cached
            onCalibrated?.invoke(cached, TtsSettingsStore.DEFAULT_VI_SYLLABLES_PER_SEC)
            return
        }
        val engine = tts ?: return
        if (calibrationInProgress) return
        calibrationInProgress = true
        val probeFile = File(cacheDir, "tts_vi_calibration_probe.wav")
        val savedRate = calibratedBaseSpeechRate
        calibratedBaseSpeechRate = 1.0f
        synthesizeCueToWav(
            cueId = "__vi_calibration_probe__",
            text = TtsSettingsStore.CALIBRATION_PROBE_TEXT_VI,
            outFile = probeFile,
            onReady = { durationMs ->
                val durSec = (durationMs.coerceAtLeast(1000L)) / 1000f
                val rawSylPerSec = TtsSettingsStore.CALIBRATION_PROBE_SYLLABLES / durSec
                val computedRate = (TtsSettingsStore.DEFAULT_VI_SYLLABLES_PER_SEC / rawSylPerSec)
                    .coerceIn(0.65f, 1.65f)
                calibratedBaseSpeechRate = computedRate
                // Verification pass: re-measure at `computedRate` to correct non-linear Android TTS rate curves.
                val verifyFile = File(cacheDir, "tts_vi_calibration_verify.wav")
                synthesizeCueToWav(
                    cueId = "__vi_calibration_verify__",
                    text = TtsSettingsStore.CALIBRATION_PROBE_TEXT_VI,
                    outFile = verifyFile,
                    onReady = { verifyDurMs ->
                        calibrationInProgress = false
                        val vSec = (verifyDurMs.coerceAtLeast(1000L)) / 1000f
                        val verifiedSylPerSec = TtsSettingsStore.CALIBRATION_PROBE_SYLLABLES / vSec
                        val refinedRate = (computedRate * (TtsSettingsStore.DEFAULT_VI_SYLLABLES_PER_SEC / verifiedSylPerSec))
                            .coerceIn(0.65f, 1.65f)
                        calibratedBaseSpeechRate = refinedRate
                        settingsStore.saveCalibratedBaseSpeechRate(voiceKey, refinedRate, verifiedSylPerSec)
                        try { if (probeFile.exists()) probeFile.delete() } catch (_: Exception) {}
                        try { if (verifyFile.exists()) verifyFile.delete() } catch (_: Exception) {}
                        LabLogStore.log(
                            "MATCH_TTS",
                            "Calibrated+verified VI base rate=$refinedRate (raw=$rawSylPerSec, verified=$verifiedSylPerSec syl/s)"
                        )
                        onCalibrated?.invoke(refinedRate, verifiedSylPerSec)
                    },
                    onError = { _ ->
                        calibrationInProgress = false
                        settingsStore.saveCalibratedBaseSpeechRate(voiceKey, computedRate, rawSylPerSec)
                        try { if (probeFile.exists()) probeFile.delete() } catch (_: Exception) {}
                        try { if (verifyFile.exists()) verifyFile.delete() } catch (_: Exception) {}
                        onCalibrated?.invoke(computedRate, rawSylPerSec)
                    }
                )
            },
            onError = { _ ->
                calibrationInProgress = false
                calibratedBaseSpeechRate = savedRate
                try { if (probeFile.exists()) probeFile.delete() } catch (_: Exception) {}
            }
        )
    }

    fun getCalibratedBaseSpeechRate(): Float = calibratedBaseSpeechRate

    private fun dispatchSynthRequest(request: SynthRequest) {
        val engine = tts ?: run {
            mainHandler.post { request.onError("tts_engine_null") }
            return
        }
        try {
            request.outFile.parentFile?.mkdirs()
            // Normalize synthesis rate to 1.0x for canonical cue measurement; playback speed is
            // controlled solely by MediaPlayer + PlaybackParams(speed=rate, pitch=1.0f).
            // CRITICAL-05: Use measured voice-calibrated speech rate targeting 4.2 syllables/sec at 1.0x.
            engine.setSpeechRate(calibratedBaseSpeechRate.coerceIn(0.65f, 1.65f))
            engine.setPitch(selection.pitchPercent / 100f)

            val params = Bundle().apply {
                putFloat(TextToSpeech.Engine.KEY_PARAM_VOLUME, selection.volumePercent / 100f)
            }
            synthRequested.incrementAndGet()
            activeSynthRequests[request.utteranceId] = request

            val timeoutRunnable = Runnable {
                val expired = activeSynthRequests.remove(request.utteranceId) ?: return@Runnable
                synthError.incrementAndGet()
                val msg = "synthesizeCueToWav timeout cueId=${expired.cueId}"
                LabLogStore.log("MATCH_TTS", msg)
                updateHealth(TtsHealthStatus.BACKEND_ERROR, msg)
                expired.onError(msg)
            }
            request.timeoutRunnable = timeoutRunnable
            mainHandler.postDelayed(timeoutRunnable, SYNTHESIS_TIMEOUT_MS)

            val result = engine.synthesizeToFile(request.text, params, request.outFile, request.utteranceId)
            if (result == TextToSpeech.ERROR) {
                activeSynthRequests.remove(request.utteranceId)
                mainHandler.removeCallbacks(timeoutRunnable)
                synthError.incrementAndGet()
                val msg = "synthesizeToFile returned ERROR cueId=${request.cueId}"
                LabLogStore.log("MATCH_TTS", msg)
                updateHealth(TtsHealthStatus.BACKEND_ERROR, msg)
                mainHandler.post { request.onError(msg) }
            }
        } catch (error: RuntimeException) {
            activeSynthRequests.remove(request.utteranceId)
            request.timeoutRunnable?.let { mainHandler.removeCallbacks(it) }
            synthError.incrementAndGet()
            val msg = "synthesizeCueToWav threw ${error.javaClass.simpleName}: ${error.message}"
            LabLogStore.error("MATCH_TTS", msg, error)
            updateHealth(TtsHealthStatus.BACKEND_ERROR, msg)
            mainHandler.post { request.onError(msg) }
        }
    }

    /**
     * Plays a pre-synthesized WAV file with pitch preservation (`pitch = 1.0f`) at any supported
     * playback rate (`1.0x / 1.25x / 1.5x / 2.0x / 3.0x`) using built-in `MediaPlayer` + `PlaybackParams`.
     */
    fun playWavAtRate(
        wavFile: File,
        playbackRate: Float,
        startOffsetMs: Long = 0L,
        onComplete: () -> Unit,
        onError: (String) -> Unit
    ) {
        if (released || !selection.enabled) {
            mainHandler.post { onError(if (released) "tts_released" else "tts_disabled") }
            return
        }
        if (!wavFile.exists() || wavFile.length() <= 44L) {
            mainHandler.post { onError("wav_missing_or_empty:${wavFile.name}") }
            return
        }
        stopWavPlayback()

        val clampedRate = playbackRate.coerceIn(0.5f, 3.0f)
        activePlaybackRate = clampedRate
        try {
            val mp = MediaPlayer()
            mp.setAudioAttributes(
                AudioAttributes.Builder()
                    .setUsage(AudioAttributes.USAGE_MEDIA)
                    .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
                    .build()
            )
            val vol = selection.volumePercent / 100f
            mp.setVolume(vol, vol)
            mp.setDataSource(wavFile.absolutePath)
            mp.prepare()

            if (startOffsetMs > 0L) {
                val safeSeek = startOffsetMs.toInt().coerceAtMost((mp.duration - 50).coerceAtLeast(0))
                if (safeSeek > 0) mp.seekTo(safeSeek)
            }

            mp.setOnCompletionListener { player ->
                releasePlayerInstance(player)
                mainHandler.post { onComplete() }
            }
            mp.setOnErrorListener { player, what, extra ->
                releasePlayerInstance(player)
                val msg = "MediaPlayer error what=$what extra=$extra file=${wavFile.name}"
                LabLogStore.log("MATCH_TTS", msg)
                updateHealth(TtsHealthStatus.BACKEND_ERROR, msg)
                mainHandler.post { onError(msg) }
                true
            }

            val params = PlaybackParams()
                .setSpeed(clampedRate)
                .setPitch(1.0f)
                .setAudioFallbackMode(PlaybackParams.AUDIO_FALLBACK_MODE_DEFAULT)
            mp.playbackParams = params
            if (!mp.isPlaying) {
                mp.start()
            }

            synchronized(wavPlayerLock) {
                wavPlayer = mp
                wavPlaying = true
                wavPaused = false
            }
        } catch (error: Exception) {
            stopWavPlayback()
            val msg = "playWavAtRate failed: ${error.javaClass.simpleName}: ${error.message}"
            LabLogStore.error("MATCH_TTS", msg, error)
            updateHealth(TtsHealthStatus.BACKEND_ERROR, msg)
            mainHandler.post { onError(msg) }
        }
    }

    /**
     * Dynamically updates the speed of the currently playing WAV cue while keeping pitch = 1.0f,
     * preserving 100% of the cue text and audio without cutting off the sentence.
     */
    fun updateActivePlaybackRate(playbackRate: Float) {
        val clampedRate = playbackRate.coerceIn(0.5f, 3.0f)
        activePlaybackRate = clampedRate
        synchronized(wavPlayerLock) {
            val mp = wavPlayer ?: return
            if (!wavPlaying) return
            try {
                val wasPaused = wavPaused
                val params = PlaybackParams()
                    .setSpeed(clampedRate)
                    .setPitch(1.0f)
                    .setAudioFallbackMode(PlaybackParams.AUDIO_FALLBACK_MODE_DEFAULT)
                mp.playbackParams = params
                if (wasPaused && mp.isPlaying) {
                    mp.pause()
                }
            } catch (error: RuntimeException) {
                LabLogStore.error("MATCH_TTS", "updateActivePlaybackRate($clampedRate) failed", error)
            }
        }
    }

    fun pauseWavPlayback() {
        synchronized(wavPlayerLock) {
            val mp = wavPlayer ?: return
            if (wavPlaying && !wavPaused) {
                try {
                    if (mp.isPlaying) mp.pause()
                    wavPaused = true
                } catch (_: RuntimeException) {
                }
            }
        }
    }

    fun resumeWavPlayback() {
        synchronized(wavPlayerLock) {
            val mp = wavPlayer ?: return
            if (wavPlaying && wavPaused) {
                try {
                    mp.start()
                    wavPaused = false
                } catch (_: RuntimeException) {
                }
            }
        }
    }

    fun stopWavPlayback() {
        val toRelease: MediaPlayer?
        synchronized(wavPlayerLock) {
            toRelease = wavPlayer
            wavPlayer = null
            wavPlaying = false
            wavPaused = false
        }
        if (toRelease != null) {
            try {
                if (toRelease.isPlaying) toRelease.stop()
            } catch (_: RuntimeException) {
            }
            try {
                toRelease.reset()
                toRelease.release()
            } catch (_: RuntimeException) {
            }
        }
    }

    fun isWavPlaying(): Boolean = wavPlaying && !wavPaused

    fun currentWavPositionMs(): Long {
        synchronized(wavPlayerLock) {
            val mp = wavPlayer ?: return 0L
            return try {
                mp.currentPosition.toLong().coerceAtLeast(0L)
            } catch (_: RuntimeException) {
                0L
            }
        }
    }

    private fun releasePlayerInstance(player: MediaPlayer) {
        synchronized(wavPlayerLock) {
            if (wavPlayer === player) {
                wavPlayer = null
                wavPlaying = false
                wavPaused = false
            }
        }
        try {
            player.reset()
            player.release()
        } catch (_: RuntimeException) {
        }
    }

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
        try {
            engine.setSpeechRate(selection.ratePercent / 100f)
            engine.setPitch(selection.pitchPercent / 100f)
            val result = engine.speak(text, TextToSpeech.QUEUE_FLUSH, parameters, utteranceId)
            LabLogStore.log(
                "MATCH_TTS",
                "speak result=$result chars=${text.length} volume=${selection.volumePercent} id=$utteranceId"
            )
            if (result == TextToSpeech.ERROR) {
                ttsError.incrementAndGet()
                updateHealth(TtsHealthStatus.BACKEND_ERROR, "speak returned ERROR")
                finishUtterance(utteranceId)
                return
            }
            val estimatedWait = 1200L + Math.round((text.length * 90L) * (100f / maxOf(50, selection.ratePercent)))
            val maximumWait = estimatedWait.coerceIn(2500L, 30000L)
            timeout = Runnable {
                ttsTimedOut.incrementAndGet()
                updateHealth(TtsHealthStatus.BACKEND_ERROR, "utterance timeout id=$utteranceId")
                LabLogStore.log("MATCH_TTS", "utterance timeout id=$utteranceId")
                finishUtterance(utteranceId)
            }.also { mainHandler.postDelayed(it, maximumWait) }
        } catch (error: RuntimeException) {
            ttsError.incrementAndGet()
            updateHealth(TtsHealthStatus.BACKEND_ERROR, "speak threw ${error.javaClass.simpleName}")
            LabLogStore.error("MATCH_TTS", "speak threw exception id=$utteranceId", error)
            finishUtterance(utteranceId)
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
        stopWavPlayback()
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
        failAllPendingSyntheses("tts_released")
        for ((_, req) in activeSynthRequests) {
            req.timeoutRunnable?.let { mainHandler.removeCallbacks(it) }
        }
        activeSynthRequests.clear()
        stopWavPlayback()
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
        private const val SYNTHESIS_TIMEOUT_MS = 12_000L

        /**
         * Reads exact WAV duration directly from the RIFF/WAVE header (`dataSize * 1000L / byteRate`),
         * falling back to `MediaMetadataRetriever`, and finally to Vietnamese syllable estimation.
         */
        fun measureWavDurationMs(
            wavFile: File,
            fallbackText: String = "",
            syllablesPerSec: Float = TtsSettingsStore.DEFAULT_VI_SYLLABLES_PER_SEC
        ): Long {
            parseRiffWaveDurationMs(wavFile)?.let { if (it > 0L) return it }
            extractMediaMetadataDurationMs(wavFile)?.let { if (it > 0L) return it }
            return TtsSettingsStore.estimateVietnameseSpeechDurationMs(fallbackText, syllablesPerSec)
                .coerceAtLeast(600L)
        }

        /**
         * Parses RIFF/WAVE header (44-byte canonical layout plus chunk-walking for optional metadata chunks).
         * Returns `(dataSize * 1000L) / byteRate` in milliseconds or null if unreadable/invalid.
         */
        fun parseRiffWaveDurationMs(wavFile: File): Long? {
            if (!wavFile.exists() || wavFile.length() < 44L) return null
            return try {
                RandomAccessFile(wavFile, "r").use { raf ->
                    val fileLen = raf.length()
                    val header = ByteArray(12)
                    raf.readFully(header)
                    if (String(header, 0, 4, Charsets.US_ASCII) != "RIFF" ||
                        String(header, 8, 4, Charsets.US_ASCII) != "WAVE"
                    ) {
                        return null
                    }
                    var byteRate = 0L
                    var dataSize = -1L
                    var offset = 12L
                    val chunkHeader = ByteArray(8)
                    while (offset + 8L <= fileLen) {
                        raf.seek(offset)
                        raf.readFully(chunkHeader)
                        val chunkId = String(chunkHeader, 0, 4, Charsets.US_ASCII)
                        val chunkSize = ByteBuffer.wrap(chunkHeader, 4, 4)
                            .order(ByteOrder.LITTLE_ENDIAN)
                            .int.toLong() and 0xFFFFFFFFL
                        if (chunkId == "fmt " && chunkSize >= 16L && offset + 8L + 16L <= fileLen) {
                            val fmt = ByteArray(16)
                            raf.readFully(fmt)
                            byteRate = ByteBuffer.wrap(fmt, 8, 4)
                                .order(ByteOrder.LITTLE_ENDIAN)
                                .int.toLong() and 0xFFFFFFFFL
                        } else if (chunkId == "data") {
                            val remaining = (fileLen - (offset + 8L)).coerceAtLeast(0L)
                            dataSize = if (chunkSize in 1L..remaining) chunkSize else remaining
                            if (byteRate > 0L) break
                        }
                        val paddedSize = chunkSize + (chunkSize and 1L)
                        offset += 8L + paddedSize
                    }
                    if (byteRate > 0L && dataSize > 0L) {
                        ((dataSize * 1000L) / byteRate).coerceAtLeast(1L)
                    } else {
                        null
                    }
                }
            } catch (_: Exception) {
                null
            }
        }

        private fun extractMediaMetadataDurationMs(wavFile: File): Long? {
            if (!wavFile.exists() || wavFile.length() <= 44L) return null
            val retriever = MediaMetadataRetriever()
            return try {
                retriever.setDataSource(wavFile.absolutePath)
                val raw = retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_DURATION)
                raw?.toLongOrNull()?.takeIf { it > 0L }
            } catch (_: Exception) {
                null
            } finally {
                try {
                    retriever.release()
                } catch (_: Exception) {
                }
            }
        }

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
