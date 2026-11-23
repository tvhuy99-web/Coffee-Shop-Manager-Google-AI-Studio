package pl.mkrstudio.truefootball3.lab

import android.content.Context
import android.os.Handler
import android.os.Looper
import org.json.JSONArray
import org.json.JSONObject
import pl.mkrstudio.truefootball3.audio.MatchTtsManager
import pl.mkrstudio.truefootball3.audio.TtsSettingsStore
import kotlin.math.roundToLong

/**
 * Hardened V4 playback coordinator with:
 * - Dynamic per-step reload of `TtsSettingsStore` & `SimulationSettingsStore` (live speed & mode changes)
 * - `pause()`, `resume()`, and `setLiveRateOverride()` controls
 * - `sessionGeneration` token guarding against stale TTS `onDone` callbacks
 * - Per-utterance safety timeout so a stalled Android TTS engine never freezes the match
 */
class LabV4PlaybackCoordinator(
    private val context: Context,
    private val ttsManager: MatchTtsManager?
) {
    private val mainHandler = Handler(Looper.getMainLooper())
    private var pendingStep: Runnable? = null
    private var safetyTimeout: Runnable? = null
    private var stopped = false
    private var paused = false
    private var sessionGeneration: Long = 0L
    private var resumeTask: (() -> Unit)? = null
    @Volatile private var liveRateOverride: Float? = null

    companion object {
        private const val BASE_PAUSE_NORMAL_MS = 650L
        private const val BASE_PAUSE_HIGHLIGHT_MS = 1150L
        private const val MIN_AUDIO_GAP_MS = 120L
        private const val BASE_TEXT_READ_MS_PER_CHAR = 52f

        fun filterEventsForViewingMode(
            allEvents: JSONArray,
            viewingMode: LabViewingMode,
            includeTurningPointsInShort: Boolean = true
        ): List<JSONObject> {
            val list = ArrayList<JSONObject>(allEvents.length())
            for (i in 0 until allEvents.length()) {
                list.add(allEvents.getJSONObject(i))
            }
            if (viewingMode == LabViewingMode.FULL_MATCH) {
                return list
            }

            val includedIndices = HashSet<Int>()
            val includedSeqIds = HashSet<Int>()

            for (i in list.indices) {
                val ev = list[i]
                val kind = ev.optString("kind", "")
                val tier = ev.optString("highlightTier", "NONE")
                val seqId = if (ev.has("highlightSequenceId") && !ev.isNull("highlightSequenceId")) {
                    ev.optInt("highlightSequenceId", -1)
                } else {
                    -1
                }

                if (kind == "MATCH_STARTED" || kind == "HALF_TIME" || kind == "FULL_TIME" ||
                    kind == "EXTRA_TIME_STARTED" || kind == "PENALTY_SHOOTOUT_STARTED" ||
                    kind == "PENALTY_SHOOTOUT_ENDED"
                ) {
                    includedIndices.add(i)
                    continue
                }

                val matches = when (viewingMode) {
                    LabViewingMode.FULL_MATCH -> true
                    LabViewingMode.EXTENDED_HIGHLIGHTS -> tier == "EXTENDED" || tier == "GOAL_KEY"
                    LabViewingMode.SHORT_HIGHLIGHTS -> {
                        if (kind == "GOAL" || kind == "PENALTY_SHOOTOUT_KICK") {
                            true
                        } else if (includeTurningPointsInShort &&
                            (kind == "PENALTY_AWARDED" || kind == "PENALTY_KICK" || kind == "RED_CARD")
                        ) {
                            true
                        } else {
                            false
                        }
                    }
                }

                if (matches) {
                    includedIndices.add(i)
                    if (seqId >= 0) includedSeqIds.add(seqId)
                    val leadIn = if (viewingMode == LabViewingMode.EXTENDED_HIGHLIGHTS) 2 else 1
                    for (prev in maxOf(0, i - leadIn) until i) {
                        includedIndices.add(prev)
                    }
                }
            }

            if (includedSeqIds.isNotEmpty()) {
                for (i in list.indices) {
                    val ev = list[i]
                    if (ev.has("highlightSequenceId") && !ev.isNull("highlightSequenceId")) {
                        if (ev.optInt("highlightSequenceId", -1) in includedSeqIds) {
                            includedIndices.add(i)
                        }
                    }
                }
            }

            return list.indices.filter { it in includedIndices }.map { list[it] }
        }

        fun computeScaledPauseMs(isHighlight: Boolean, ttsRateMultiplier: Float, syncWithTts: Boolean): Long {
            val base = if (isHighlight) BASE_PAUSE_HIGHLIGHT_MS else BASE_PAUSE_NORMAL_MS
            if (!syncWithTts) return base
            val rate = ttsRateMultiplier.coerceIn(0.5f, 5.0f)
            return (base / rate).roundToLong().coerceAtLeast(MIN_AUDIO_GAP_MS)
        }
    }

    fun setLiveRateOverride(rateMultiplier: Float?) {
        liveRateOverride = rateMultiplier?.coerceIn(0.5f, 5.0f)
    }

    fun pause() {
        paused = true
        cancelPending()
        ttsManager?.stop()
    }

    fun resume() {
        if (!paused || stopped) return
        paused = false
        val task = resumeTask
        resumeTask = null
        task?.invoke()
    }

    fun isPaused(): Boolean = paused

    fun playSequence(
        events: List<JSONObject>,
        renderText: (JSONObject) -> String,
        onEventDisplayed: (event: JSONObject, text: String, index: Int, total: Int) -> Unit,
        onFinished: () -> Unit
    ) {
        stop()
        stopped = false
        paused = false
        val gen = ++sessionGeneration
        if (events.isEmpty()) {
            onFinished()
            return
        }

        fun step(index: Int) {
            if (stopped || gen != sessionGeneration) return
            if (paused) {
                resumeTask = { step(index) }
                return
            }
            if (index >= events.size) {
                onFinished()
                return
            }

            // Reload TTS & Simulation settings dynamically on EVERY step so live changes take effect immediately
            val simSettings = SimulationSettingsStore(context).load()
            val ttsSettings = TtsSettingsStore(context).load()
            val ttsRate = (liveRateOverride ?: (ttsSettings.ratePercent / 100f)).coerceIn(0.5f, 5.0f)
            val useTts = ttsSettings.enabled && ttsManager != null && ttsManager.isEnabled()

            val ev = events[index]
            val line = renderText(ev)
            val tier = ev.optString("highlightTier", "NONE")
            val isHighlight = tier == "EXTENDED" || tier == "GOAL_KEY"
            val pauseMs = computeScaledPauseMs(isHighlight, ttsRate, simSettings.syncMatchSpeedWithTts)

            onEventDisplayed(ev, line, index, events.size)

            if (useTts && line.isNotBlank()) {
                var advanced = false
                fun advanceOnce() {
                    if (advanced || stopped || gen != sessionGeneration) return
                    advanced = true
                    cancelSafetyTimeout()
                    if (paused) {
                        resumeTask = { step(index + 1) }
                        return
                    }
                    val nextTask = Runnable { step(index + 1) }
                    pendingStep = nextTask
                    mainHandler.postDelayed(nextTask, pauseMs)
                }

                // Safety timeout prevents match stall if Android TTS fails to fire onDone
                val estMaxMs = (1600L + (line.length * 85f / ttsRate).roundToLong()).coerceIn(2500L, 25000L)
                val guard = Runnable { advanceOnce() }
                safetyTimeout = guard
                mainHandler.postDelayed(guard, estMaxMs)

                ttsManager?.speak(line, Runnable { advanceOnce() })
            } else {
                val readMs = ((line.length * BASE_TEXT_READ_MS_PER_CHAR) / ttsRate)
                    .roundToLong()
                    .coerceIn(220L, 4500L)
                val nextTask = Runnable { step(index + 1) }
                pendingStep = nextTask
                mainHandler.postDelayed(nextTask, readMs + pauseMs)
            }
        }

        step(0)
    }

    private fun cancelSafetyTimeout() {
        safetyTimeout?.let { mainHandler.removeCallbacks(it) }
        safetyTimeout = null
    }

    private fun cancelPending() {
        pendingStep?.let { mainHandler.removeCallbacks(it) }
        pendingStep = null
        cancelSafetyTimeout()
    }

    fun stop() {
        stopped = true
        paused = false
        sessionGeneration++
        resumeTask = null
        cancelPending()
        ttsManager?.stop()
    }
}
