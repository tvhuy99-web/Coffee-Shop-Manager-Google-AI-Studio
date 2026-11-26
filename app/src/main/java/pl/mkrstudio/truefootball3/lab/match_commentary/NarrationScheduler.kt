package pl.mkrstudio.truefootball3.match_commentary

import pl.mkrstudio.truefootball3.lab.*
import pl.mkrstudio.truefootball3.match_playback.MatchPlaybackController

import android.content.Context
import android.os.Handler
import android.os.Looper
import pl.mkrstudio.truefootball3.match_commentary.MatchTtsManager
import pl.mkrstudio.truefootball3.audio.TtsSettingsStore
import java.io.File
import java.util.concurrent.atomic.AtomicLong

/**
 * DES v1.4 1.0x-Canonical Narration Scheduler & Lookahead WAV Ring-Buffer Manager.
 *
 * Core guarantees:
 * 1. Strict separation between raw [SimulationEvent] and locked [CommentaryCue].
 * 2. Coalesces dense events and locks a non-overlapping cue timeline at canonical `1.0x` using
 *    actual WAV durations from `TextToSpeech.synthesizeToFile(...)` (initial fallback ~4.2 Vietnamese
 *    syllables/sec).
 * 3. **1.0x–3.0x Invariance**: changing playback speed (`1.0x / 1.25x / 1.5x / 2.0x / 3.0x`) keeps
 *    100% of `cueId`, `text`, and order unchanged, updating `MediaPlayer` pitch-preserving speed
 *    (`pitch = 1.0f`) live. Never mutes TTS or drops sentences at `3.0x` — only triggers
 *    [onHighSpeedWarning] with `"Ở 3.0x lời bình có thể khó nghe"`.
 * 4. Lookahead buffer (`10_000..=20_000 ms`) tagged with `generationToken: Long` so speculative cues
 *    are cleanly cancelled on early substitutions or `HalfTimeBreak` without mutating played cues.
 * 5. WAV ring-buffer inside `context.cacheDir/des_tts_wav/` auto-deletes played/cancelled WAVs and
 *    caps storage to at most `25` files.
 * 6. Backlog policy: TTS NEVER blocks the simulation clock. Slow synthesis or backend errors report
 *    [TtsHealthStatus.DEGRADED_BACKLOG] / [TtsHealthStatus.BACKEND_ERROR] transparently while text
 *    and match clock keep advancing smoothly.
 */
class NarrationScheduler(
    private val context: Context,
    private val ttsManager: MatchTtsManager?,
    lookaheadWindowMs: Long = DEFAULT_LOOKAHEAD_MS,
    private val onCuePresented: (cue: CommentaryCue, event: SimulationEvent) -> Unit = { _, _ -> },
    private val onHighSpeedWarning: (message: String) -> Unit = {},
    private val onHealthChanged: (status: TtsHealthStatus, detail: String) -> Unit = { _, _ -> }
) : pl.mkrstudio.truefootball3.presentation.MatchCommentaryBoundary {
    /**
     * Immutable simulation event arriving from the Rust DES session before cue coalescing/locking.
     */
    data class SimulationEvent(
        val sequence: Int,
        val simTimeMs: Long,
        val period: Int,
        val periodElapsedMs: Long,
        val minute: Int,
        val displayMinute: String,
        val kind: LabEventType,
        val side: LabTeamSide?,
        val homeGoals: Int,
        val awayGoals: Int,
        val text: String,
        val highlightTier: String = "NONE",
        val isWoodwork: Boolean = false,
        val isPenalty: Boolean = false,
        val isShootoutScored: Boolean? = null
    )

    data class ActiveCueCheckpointInfo(
        val lastCompletedCueId: String?,
        val inProgressCueId: String?,
        val inProgressCueAnchorSimMs: Long?,
        val inProgressCueText: String?,
        val inProgressCueOffsetMs: Long
    )

    private data class ScheduledCueEntry(
        val cueId: String,
        val eventIds: MutableList<Int>,
        val anchorEvent: SimulationEvent,
        val text: String,
        var anchorSimTimeMs: Long,
        var durationAudioMs: Long,
        val priority: CuePriority,
        var state: CueState,
        val generationToken: Long,
        val wavFile: File,
        var textPresented: Boolean = false,
        var audioStarted: Boolean = false,
        var audioFinished: Boolean = false,
        var synthFailed: Boolean = false,
        var resumeOffsetMs: Long = 0L
    ) {
        fun toPublicCue(): CommentaryCue = CommentaryCue(
            cueId = cueId,
            eventIds = eventIds.toList(),
            text = text,
            anchorSimTimeMs = anchorSimTimeMs,
            durationAudioMs = durationAudioMs,
            priority = priority,
            generationToken = generationToken,
            state = state,
            wavFilePath = if (wavFile.exists()) wavFile.absolutePath else null
        )
    }

    private val mainHandler = Handler(Looper.getMainLooper())
    private val wavCacheDir = File(File(context.cacheDir, WAV_CACHE_SUBDIR), java.util.UUID.randomUUID().toString()).apply { mkdirs() }
    private val cueIdCounter = AtomicLong(0L)
    private val lock = Any()

    val lookaheadMs: Long = lookaheadWindowMs.coerceIn(LOOKAHEAD_MIN_MS, LOOKAHEAD_MAX_MS)

    private val entries = ArrayList<ScheduledCueEntry>()
    private var activePlayingCueId: String? = null
    private var lastCompletedCueId: String? = null
    @Volatile private var generationToken: Long = 1L
    @Volatile private var playbackRate: Float = 1.0f
    @Volatile private var lastClockSimTimeMs: Long = 0L
    @Volatile private var highSpeedWarningShown: Boolean = false
    @Volatile private var paused: Boolean = false
    @Volatile private var released: Boolean = false
    @Volatile private var healthStatus: TtsHealthStatus = TtsHealthStatus.HEALTHY

    init {
        ttsManager?.setHealthListener { status, detail ->
            healthStatus = status
            onHealthChanged(status, detail)
        }
    }

    fun currentGenerationToken(): Long = generationToken

    fun currentPlaybackRate(): Float = playbackRate

    fun currentHealthStatus(): TtsHealthStatus = healthStatus

    fun exportCheckpointCueInfo(): ActiveCueCheckpointInfo = synchronized(lock) {
        val active = activePlayingCueId?.let { id -> entries.firstOrNull { it.cueId == id } }
        ActiveCueCheckpointInfo(
            lastCompletedCueId = lastCompletedCueId,
            inProgressCueId = active?.cueId,
            inProgressCueAnchorSimMs = active?.anchorSimTimeMs,
            inProgressCueText = active?.text,
            inProgressCueOffsetMs = if (active != null) (ttsManager?.currentWavPositionMs() ?: 0L) else 0L
        )
    }

    /**
     * Updates playback speed (`1.0x / 1.25x / 1.5x / 2.0x / 3.0x`) WITHOUT altering the locked
     * 1.0x cue list, text, or order, and without ever disabling TTS.
     */
    override fun setPlaybackRate(rate: Float) {
        val clamped = rate.coerceIn(MIN_PLAYBACK_RATE, MAX_PLAYBACK_RATE)
        val previous = playbackRate
        playbackRate = clamped
        ttsManager?.updateActivePlaybackRate(clamped)
        if (clamped >= HIGH_SPEED_WARNING_THRESHOLD) {
            if (!highSpeedWarningShown || previous < HIGH_SPEED_WARNING_THRESHOLD) {
                highSpeedWarningShown = true
                mainHandler.post { onHighSpeedWarning(HIGH_SPEED_WARNING_VI) }
            }
        } else {
            highSpeedWarningShown = false
        }
        LabLogStore.log("NARRATION", "setPlaybackRate -> ${clamped}x (lockedCues=${lockedCueCount()})")
    }

    /**
     * Accepts a batch of raw [SimulationEvent]s (either committed or speculative lookahead),
     * coalesces dense low-priority events, locks non-overlapping 1.0x [CommentaryCue]s, and
     * kicks off offline WAV synthesis into the bounded ring buffer.
     */
    fun ingestSimulationEvents(events: List<SimulationEvent>, isSpeculative: Boolean = true): List<CommentaryCue> {
        if (released || events.isEmpty()) return emptyList()

        val newlyLocked = ArrayList<ScheduledCueEntry>()
        val syllablesPerSec = ttsManager?.currentSelection()?.targetSyllablesPerSec
            ?: TtsSettingsStore.DEFAULT_VI_SYLLABLES_PER_SEC

        synchronized(lock) {
            val token = generationToken
            val sorted = events
                .filter { it.text.isNotBlank() }
                .sortedWith(compareBy<SimulationEvent> { it.simTimeMs }.thenBy { it.sequence })

            val coalesced = coalesceDenseEvents(sorted)

            for (group in coalesced) {
                val primary = group.primary
                val priority = priorityForEvent(primary.kind)
                val estDurationMs = TtsSettingsStore.estimateVietnameseSpeechDurationMs(
                    primary.text,
                    syllablesPerSec
                )

                val lastActive = entries.lastOrNull { it.state != CueState.CANCELLED }
                var scheduledStartMs = primary.simTimeMs
                if (lastActive != null) {
                    val lastEndMs = lastActive.anchorSimTimeMs + lastActive.durationAudioMs + MIN_CUE_GAP_MS
                    if (scheduledStartMs < lastEndMs) {
                        if (priority == CuePriority.P0_CRITICAL || priority == CuePriority.P1_HIGH) {
                            scheduledStartMs = lastEndMs
                        } else {
                            // Select the canonical narration before locking a new cue. Raw events
                            // remain available to the presentation stream; no speed-dependent drops.
                            continue
                        }
                    }
                }

                val cueId = "cue_${token}_${cueIdCounter.incrementAndGet()}"
                val wavFile = File(wavCacheDir, "$cueId.wav")
                val entry = ScheduledCueEntry(
                    cueId = cueId,
                    eventIds = group.eventIds.toMutableList(),
                    anchorEvent = primary,
                    text = primary.text.trim(),
                    anchorSimTimeMs = scheduledStartMs,
                    durationAudioMs = estDurationMs,
                    priority = priority,
                    state = if (isSpeculative) CueState.PLANNED else CueState.READY,
                    generationToken = token,
                    wavFile = wavFile
                )
                entries.add(entry)
                newlyLocked.add(entry)
            }
        }

        for (entry in newlyLocked) {
            triggerSynthesis(entry)
        }
        return newlyLocked.map { it.toPublicCue() }
    }

    /**
     * Driven by [MatchPlaybackController] on every clock tick.
     * Presents any due cues immediately (text + audio director) and plays their WAV at current
     * [playbackRate]. TTS synthesis/playback lag NEVER blocks the simulation clock.
     */
    override fun onClockTick(committedSimTimeMs: Long) {
        if (released || paused) return
        lastClockSimTimeMs = committedSimTimeMs

        val dueForText = ArrayList<Pair<CommentaryCue, SimulationEvent>>()

        synchronized(lock) {
            for (entry in entries) {
                if (entry.state == CueState.CANCELLED) continue
                if (!entry.textPresented && committedSimTimeMs >= entry.anchorSimTimeMs) {
                    entry.textPresented = true
                    dueForText.add(entry.toPublicCue() to entry.anchorEvent)
                }
            }


        }

        for ((cue, ev) in dueForText) {
            onCuePresented(cue, ev)
        }

        kickNextPendingAudio()
    }

    /**
     * Revokes all unplayed speculative cues scheduled after [committedSimTimeMs] (e.g. when a live
     * substitution/tactical command invalidates lookahead or when reaching `HalfTimeBreak`),
     * incrementing [generationToken] without touching already-played cues.
     */
    fun revokeSpeculativeAfter(committedSimTimeMs: Long, reason: String) {
        var revokedCount = 0
        synchronized(lock) {
            generationToken++
            for (entry in entries) {
                if (entry.state == CueState.CANCELLED ||
                    entry.state == CueState.COMPLETED ||
                    entry.state == CueState.PLAYING ||
                    entry.textPresented
                ) {
                    continue
                }
                if (entry.anchorSimTimeMs > committedSimTimeMs) {
                    cancelEntryLocked(entry, reason)
                    revokedCount++
                }
            }
        }
        if (revokedCount > 0) {
            LabLogStore.log(
                "NARRATION",
                "Revoked $revokedCount speculative cues after simTimeMs=$committedSimTimeMs (reason=$reason, gen=$generationToken)"
            )
        }
    }

    /**
     * Cancels unplayed cues in skipped intervals during Highlights fast-forward jumps.
     */
    fun onFastForwardSkip(newSimTimeMs: Long) {
        synchronized(lock) {
            generationToken++
            for (entry in entries) {
                if (entry.state == CueState.CANCELLED || entry.state == CueState.COMPLETED) continue
                if (!entry.textPresented && entry.anchorSimTimeMs < newSimTimeMs) {
                    cancelEntryLocked(entry, "highlights_skip")
                }
            }
        }
    }

    override fun pause() {
        paused = true
        ttsManager?.pauseWavPlayback()
    }

    override fun resume() {
        if (released) return
        paused = false
        ttsManager?.resumeWavPlayback()
        kickNextPendingAudio()
    }

    fun stop() {
        paused = true
        synchronized(lock) {
            activePlayingCueId = null
        }
        ttsManager?.stopWavPlayback()
    }

    override fun release() {
        released = true
        paused = true
        synchronized(lock) {
            activePlayingCueId = null
            for (entry in entries) {
                deleteWavQuietly(entry.wavFile)
            }
            entries.clear()
        }
        ttsManager?.stopWavPlayback()
        clearWavCache()
    }

    fun clearWavCache() {
        try {
            wavCacheDir.listFiles()?.forEach { file ->
                if (file.isFile && file.name.endsWith(".wav")) {
                    file.delete()
                }
            }
        } catch (_: Exception) {
        }
    }

    fun lockedCueCount(): Int = synchronized(lock) {
        entries.count { it.state != CueState.CANCELLED }
    }

    fun getLockedCuesSnapshot(): List<CommentaryCue> = synchronized(lock) {
        entries.filter { it.state != CueState.CANCELLED }.map { it.toPublicCue() }
    }

    fun exportLockedCueIds(): List<String> = synchronized(lock) {
        entries.filter { it.state != CueState.CANCELLED }.map { it.cueId }
    }

    fun wavCacheFileCount(): Int {
        return try {
            wavCacheDir.listFiles()?.count { it.isFile && it.name.endsWith(".wav") } ?: 0
        } catch (_: Exception) {
            0
        }
    }

    private fun triggerSynthesis(entry: ScheduledCueEntry) {
        val manager = ttsManager
        if (manager == null || !manager.isEnabled()) {
            synchronized(lock) {
                entry.synthFailed = true
                entry.state = CueState.BACKEND_FAILED
            }
            return
        }

        enforceWavRingBufferCap()

        synchronized(lock) {
            if (entry.state == CueState.CANCELLED) return
            entry.state = CueState.SYNTHESIZING
        }

        manager.synthesizeCueToWav(
            cueId = entry.cueId,
            text = entry.text,
            outFile = entry.wavFile,
            onReady = { measuredDurationMs ->
                var shouldStartNow = false
                synchronized(lock) {
                    if (entry.state == CueState.CANCELLED || released) {
                        deleteWavQuietly(entry.wavFile)
                        return@synchronized
                    }
                    entry.durationAudioMs = measuredDurationMs.coerceAtLeast(350L)
                    entry.state = CueState.READY
                    // CRITICAL-06: Reflow subsequent unplayed cues using actual measured WAV duration
                    // so 1.0x schedule never drifts or overlaps when real WAV != estimate.
                    reflowScheduleFromLocked(entry.cueId)
                    // CRITICAL-07: Only start now if this entry is the strict FIFO head unplayed cue!
                    val head = entries.firstOrNull {
                        !it.audioFinished && !it.synthFailed && it.state != CueState.CANCELLED
                    }
                    if (head?.cueId == entry.cueId && entry.textPresented && !entry.audioStarted && activePlayingCueId == null && !paused) {
                        shouldStartNow = true
                    }
                }
                if (healthStatus == TtsHealthStatus.DEGRADED_BACKLOG) {
                    healthStatus = TtsHealthStatus.HEALTHY
                    manager.reportHealthy("wav_ready:${entry.cueId}")
                }
                if (shouldStartNow) {
                    kickNextPendingAudio()
                }
            },
            onError = { reason ->
                synchronized(lock) {
                    if (entry.state != CueState.CANCELLED) {
                        entry.synthFailed = true
                        entry.state = CueState.BACKEND_FAILED
                    }
                    deleteWavQuietly(entry.wavFile)
                }
                val newStatus = if (reason == "tts_unavailable" || reason == "tts_disabled") {
                    TtsHealthStatus.UNAVAILABLE
                } else {
                    TtsHealthStatus.BACKEND_ERROR
                }
                healthStatus = newStatus
                onHealthChanged(newStatus, "synth_error:${entry.cueId}:$reason")
            }
        )
    }

    private fun tryStartAudioPlayback(entry: ScheduledCueEntry, committedSimTimeMs: Long) {
        val manager = ttsManager ?: return
        if (!manager.isEnabled() || released || paused) return

        val readyToPlay: Boolean
        val laggingBehind: Boolean
        synchronized(lock) {
            if (entry.state == CueState.CANCELLED || entry.audioStarted || entry.audioFinished || entry.synthFailed) {
                return
            }
            if (entry.state == CueState.SYNTHESIZING || entry.state == CueState.PLANNED || !entry.wavFile.exists()) {
                readyToPlay = false
                laggingBehind = (committedSimTimeMs - entry.anchorSimTimeMs) >= BACKLOG_LAG_THRESHOLD_MS
            } else {
                readyToPlay = true
                laggingBehind = false
                entry.audioStarted = true
                entry.state = CueState.PLAYING
                activePlayingCueId = entry.cueId
            }
        }

        if (!readyToPlay) {
            if (laggingBehind) {
                healthStatus = TtsHealthStatus.DEGRADED_BACKLOG
                manager.reportBacklogDegraded("Synthesis behind clock for ${entry.cueId}")
                onHealthChanged(TtsHealthStatus.DEGRADED_BACKLOG, "Synthesis lagging for ${entry.cueId}")
            }
            return
        }

        val offsetToUse = entry.resumeOffsetMs.coerceAtLeast(0L)
        entry.resumeOffsetMs = 0L
        manager.playWavAtRate(
            wavFile = entry.wavFile,
            playbackRate = playbackRate,
            startOffsetMs = offsetToUse,
            onComplete = {
                synchronized(lock) {
                    entry.audioFinished = true
                    if (entry.state != CueState.CANCELLED) {
                        entry.state = CueState.COMPLETED
                        lastCompletedCueId = entry.cueId
                    }
                    if (activePlayingCueId == entry.cueId) {
                        activePlayingCueId = null
                    }
                    deleteWavQuietly(entry.wavFile)
                }
                kickNextPendingAudio()
            },
            onError = { err ->
                synchronized(lock) {
                    entry.audioFinished = true
                    entry.synthFailed = true
                    if (entry.state != CueState.CANCELLED) {
                        entry.state = CueState.BACKEND_FAILED
                    }
                    if (activePlayingCueId == entry.cueId) {
                        activePlayingCueId = null
                    }
                    deleteWavQuietly(entry.wavFile)
                }
                healthStatus = TtsHealthStatus.BACKEND_ERROR
                onHealthChanged(TtsHealthStatus.BACKEND_ERROR, "playback_error:${entry.cueId}:$err")
                kickNextPendingAudio()
            }
        )
    }

    /**
     * CRITICAL-06: Recalculates `anchorSimTimeMs` for all unplayed cues strictly following
     * `updatedCueId` whenever a WAV finishes synthesizing with its true measured duration.
     */
    private fun reflowScheduleFromLocked(updatedCueId: String) {
        val startIdx = entries.indexOfFirst { it.cueId == updatedCueId }
        if (startIdx < 0) return
        var prevEndMs = entries[startIdx].anchorSimTimeMs + entries[startIdx].durationAudioMs + MIN_CUE_GAP_MS
        for (i in (startIdx + 1) until entries.size) {
            val cur = entries[i]
            if (cur.state == CueState.CANCELLED || cur.audioStarted || cur.audioFinished) {
                if (cur.state != CueState.CANCELLED) {
                    prevEndMs = maxOf(prevEndMs, cur.anchorSimTimeMs + cur.durationAudioMs + MIN_CUE_GAP_MS)
                }
                continue
            }
            val desiredStart = maxOf(cur.anchorEvent.simTimeMs, prevEndMs)
            cur.anchorSimTimeMs = desiredStart
            prevEndMs = desiredStart + cur.durationAudioMs + MIN_CUE_GAP_MS
        }
    }

    /**
     * CRITICAL-07: Strict FIFO audio queue dispatcher. Never plays cue N+1 while cue N is still
     * synthesizing or pending; logs `DEGRADED_BACKLOG` if the head cue lags behind the clock.
     */
    private fun kickNextPendingAudio() {
        var headToPlay: ScheduledCueEntry? = null
        var laggingCueId: String? = null
        var committedNow = 0L
        synchronized(lock) {
            if (activePlayingCueId != null || paused || released) return
            committedNow = lastClockSimTimeMs
            val head = entries.firstOrNull {
                !it.audioFinished && !it.synthFailed && it.state != CueState.CANCELLED
            } ?: return
            if (!head.textPresented && head.anchorSimTimeMs > committedNow) {
                return
            }
            if (head.state == CueState.READY && head.wavFile.exists()) {
                headToPlay = head
            } else if (committedNow - head.anchorSimTimeMs >= BACKLOG_LAG_THRESHOLD_MS) {
                laggingCueId = head.cueId
            }
        }
        if (laggingCueId != null) {
            if (healthStatus != TtsHealthStatus.DEGRADED_BACKLOG) {
                healthStatus = TtsHealthStatus.DEGRADED_BACKLOG
                onHealthChanged(
                    TtsHealthStatus.DEGRADED_BACKLOG,
                    "fifo_head_not_ready:$laggingCueId"
                )
            }
            return
        }
        headToPlay?.let { tryStartAudioPlayback(it, committedNow) }
    }

    /** CRITICAL-04 / QA-09: Exports unplayed locked 1.0x cues for checkpoint persistence. */
    fun exportPendingCuesJson(): String {
        val arr = org.json.JSONArray()
        synchronized(lock) {
            for (e in entries) {
                if (e.state == CueState.CANCELLED || e.audioFinished) continue
                arr.put(
                    org.json.JSONObject()
                        .put("cueId", e.cueId)
                        .put("text", e.text)
                        .put("anchorSimTimeMs", e.anchorSimTimeMs)
                        .put("durationAudioMs", e.durationAudioMs)
                        .put("priority", e.priority.name)
                        .put("eventSeq", e.anchorEvent.sequence)
                        .put("eventKind", e.anchorEvent.kind.name)
                        .put("eventSimTimeMs", e.anchorEvent.simTimeMs)
                        .put("period", e.anchorEvent.period)
                        .put("periodElapsedMs", e.anchorEvent.periodElapsedMs)
                        .put("minute", e.anchorEvent.minute)
                        .put("displayMinute", e.anchorEvent.displayMinute)
                        .put("homeGoals", e.anchorEvent.homeGoals)
                        .put("awayGoals", e.anchorEvent.awayGoals)
                        .put("side", e.anchorEvent.side?.name ?: org.json.JSONObject.NULL)
                        .put("eventIds", org.json.JSONArray(e.eventIds))
                        .put("textPresented", e.textPresented)
                )
            }
        }
        return arr.toString()
    }

    /** CRITICAL-04 / QA-09: Restores locked unplayed cues (with in-progress offset) and re-synthesizes WAVs. */
    fun restorePendingCuesJson(
        cuesJson: String,
        inProgressCueId: String? = null,
        inProgressOffsetMs: Long = 0L
    ) {
        if (cuesJson.isBlank() || cuesJson == "[]") return
        val restored = ArrayList<ScheduledCueEntry>()
        try {
            val arr = org.json.JSONArray(cuesJson)
            synchronized(lock) {
                val token = generationToken
                for (i in 0 until arr.length()) {
                    val obj = arr.getJSONObject(i)
                    val cueId = obj.optString("cueId", "cue_restored_$i")
                    if (entries.any { it.cueId == cueId }) continue
                    cueId.substringAfterLast('_').toLongOrNull()?.let { n ->
                        cueIdCounter.set(maxOf(cueIdCounter.get(), n))
                    }
                    val kind = try {
                        LabEventType.valueOf(obj.optString("eventKind", "PASS_COMPLETED"))
                    } catch (_: Exception) {
                        LabEventType.PASS_COMPLETED
                    }
                    val prio = try {
                        CuePriority.valueOf(obj.optString("priority", "P2_MEDIUM"))
                    } catch (_: Exception) {
                        CuePriority.P2_MEDIUM
                    }
                    val evSimMs = obj.optLong("eventSimTimeMs", obj.optLong("anchorSimTimeMs", 0L))
                    val period = obj.optInt("period", 1)
                    val minute = obj.optInt("minute", ((evSimMs / 60_000L).toInt() + 1).coerceAtLeast(1))
                    val displayMin = obj.optString("displayMinute", minute.toString())
                    val text = obj.optString("text", "")
                    val simEv = SimulationEvent(
                        sequence = obj.optInt("eventSeq", i),
                        simTimeMs = evSimMs,
                        period = period,
                        periodElapsedMs = obj.optLong("periodElapsedMs", evSimMs),
                        minute = minute,
                        displayMinute = displayMin,
                        kind = kind,
                        side = runCatching { LabTeamSide.valueOf(obj.optString("side")) }.getOrNull(),
                        homeGoals = obj.optInt("homeGoals", 0),
                        awayGoals = obj.optInt("awayGoals", 0),
                        text = text
                    )
                    val resumeOff = if (cueId == inProgressCueId) inProgressOffsetMs.coerceAtLeast(0L) else 0L
                    val entry = ScheduledCueEntry(
                        cueId = cueId,
                        eventIds = obj.optJSONArray("eventIds")?.let { ids ->
                            (0 until ids.length()).map { ids.getInt(it) }.toMutableList()
                        } ?: mutableListOf(simEv.sequence),
                        anchorEvent = simEv,
                        text = text,
                        anchorSimTimeMs = obj.optLong("anchorSimTimeMs", evSimMs),
                        durationAudioMs = obj.optLong("durationAudioMs", 1500L),
                        priority = prio,
                        state = CueState.PLANNED,
                        generationToken = token,
                        wavFile = File(wavCacheDir, "$cueId.wav"),
                        textPresented = obj.optBoolean("textPresented", false),
                        resumeOffsetMs = resumeOff
                    )
                    entries.add(entry)
                    restored.add(entry)
                }
            }
            for (e in restored) {
                triggerSynthesis(e)
            }
        } catch (_: Exception) {
        }
    }

    fun reportBoundaryBacklog(boundaryMs: Long) {
        val pending = synchronized(lock) { entries.count {
            it.anchorEvent.simTimeMs <= boundaryMs && !it.audioFinished && !it.synthFailed && it.state != CueState.CANCELLED
        } }
        if (pending > 0) {
            healthStatus = TtsHealthStatus.DEGRADED_BACKLOG
            onHealthChanged(healthStatus, "boundary_backlog:$boundaryMs:$pending")
            LabLogStore.log("NARRATION", "Boundary reached with $pending audio cues pending")
        }
    }

    /** Returns true if any locked cue is currently playing or still queued to play. */
    fun hasActiveOrPendingAudio(): Boolean = synchronized(lock) {
        activePlayingCueId != null || entries.any {
            !it.audioFinished && !it.synthFailed && it.state != CueState.CANCELLED
        }
    }

    private fun cancelEntryLocked(entry: ScheduledCueEntry, reason: String) {
        entry.state = CueState.CANCELLED
        deleteWavQuietly(entry.wavFile)
        LabLogStore.log("NARRATION", "Cue cancelled id=${entry.cueId} reason=$reason")
    }

    /**
     * CRITICAL-07: Safe ring-buffer cleanup. Strictly protects every WAV belonging to an active
     * unplayed cue (`PLANNED`, `SYNTHESIZING`, `READY`, `PLAYING`); only evicts completed,
     * cancelled, failed, or orphaned WAV files.
     */
    private fun enforceWavRingBufferCap() {
        try {
            val files = wavCacheDir.listFiles()?.filter { it.isFile && it.name.endsWith(".wav") } ?: return
            if (files.size < MAX_WAV_RING_FILES) return
            val protectedNames = HashSet<String>()
            synchronized(lock) {
                for (entry in entries) {
                    val isProtected = !entry.audioFinished &&
                        !entry.synthFailed &&
                        entry.state != CueState.CANCELLED &&
                        (entry.state == CueState.PLANNED ||
                            entry.state == CueState.SYNTHESIZING ||
                            entry.state == CueState.READY ||
                            entry.state == CueState.PLAYING)
                    if (isProtected) {
                        protectedNames.add(entry.wavFile.name)
                    }
                }
            }
            val evictable = files
                .filter { it.name !in protectedNames }
                .sortedBy { it.lastModified() }
            val toDelete = (files.size - MAX_WAV_RING_FILES + 1).coerceAtLeast(0)
            for (i in 0 until minOf(toDelete, evictable.size)) {
                evictable[i].delete()
            }
        } catch (_: Exception) {
        }
    }

    private fun deleteWavQuietly(file: File) {
        try {
            if (file.exists()) file.delete()
        } catch (_: Exception) {
        }
    }

    private data class CoalescedGroup(
        val primary: SimulationEvent,
        val eventIds: List<Int>
    )

    private fun coalesceDenseEvents(sorted: List<SimulationEvent>): List<CoalescedGroup> {
        if (sorted.isEmpty()) return emptyList()
        val result = ArrayList<CoalescedGroup>()
        var bucket = ArrayList<SimulationEvent>()
        bucket.add(sorted[0])

        fun flushBucket() {
            if (bucket.isEmpty()) return
            var best = bucket[0]
            var bestPrio = priorityForEvent(best.kind)
            for (i in 1 until bucket.size) {
                val cand = bucket[i]
                val candPrio = priorityForEvent(cand.kind)
                if (candPrio.ordinal < bestPrio.ordinal ||
                    (candPrio == bestPrio && cand.simTimeMs >= best.simTimeMs)
                ) {
                    best = cand
                    bestPrio = candPrio
                }
            }
            val mergedText = if (bucket.size == 1) {
                best.text.trim()
            } else {
                val leadIn = bucket.firstOrNull { it !== best && it.text.isNotBlank() }?.text?.trim()
                val mainText = best.text.trim()
                if (!leadIn.isNullOrBlank() && leadIn != mainText && mainText.length < 110) {
                    "${leadIn.trimEnd('.', '!', ' ')} — $mainText"
                } else {
                    mainText
                }
            }
            result.add(CoalescedGroup(best.copy(text = mergedText), bucket.map { it.sequence }))
            bucket = ArrayList()
        }

        for (i in 1 until sorted.size) {
            val ev = sorted[i]
            val first = bucket[0]
            val evPrio = priorityForEvent(ev.kind)
            val firstPrio = priorityForEvent(first.kind)
            val withinWindow = (ev.simTimeMs - first.simTimeMs) <= COALESCING_WINDOW_MS
            if (withinWindow && !(evPrio == CuePriority.P0_CRITICAL && firstPrio == CuePriority.P0_CRITICAL)) {
                bucket.add(ev)
            } else {
                flushBucket()
                bucket.add(ev)
            }
        }
        flushBucket()
        return result
    }

    companion object {
        const val WAV_CACHE_SUBDIR = "des_tts_wav"
        const val MAX_WAV_RING_FILES = 25
        const val LOOKAHEAD_MIN_MS = 10_000L
        const val LOOKAHEAD_MAX_MS = 20_000L
        const val DEFAULT_LOOKAHEAD_MS = 15_000L
        const val MAX_1X_NARRATION_LAG_MS = 12_000L
        const val MIN_PLAYBACK_RATE = 1.0f
        const val MAX_PLAYBACK_RATE = 3.0f
        const val HIGH_SPEED_WARNING_THRESHOLD = 3.0f
        const val HIGH_SPEED_WARNING_VI = "Ở 3.0x lời bình có thể khó nghe"

        private const val COALESCING_WINDOW_MS = 1_600L
        private const val MIN_CUE_GAP_MS = 220L
        private const val MAX_HIGH_PRIORITY_SHIFT_MS = 2_500L
        private const val BACKLOG_LAG_THRESHOLD_MS = 1_500L

        val SUPPORTED_PLAYBACK_RATES: FloatArray = floatArrayOf(1.0f, 1.25f, 1.5f, 2.0f, 3.0f)

        fun priorityForEvent(kind: LabEventType): CuePriority = when (kind) {
            LabEventType.GOAL,
            LabEventType.OWN_GOAL,
            LabEventType.RED_CARD,
            LabEventType.PENALTY_AWARDED,
            LabEventType.PENALTY_KICK,
            LabEventType.PENALTY_SHOOTOUT_KICK,
            LabEventType.HALF_TIME,
            LabEventType.EXTRA_TIME_HALF_TIME,
            LabEventType.PENALTY_SHOOTOUT_STARTED,
            LabEventType.PENALTY_SHOOTOUT_ENDED,
            LabEventType.FULL_TIME -> CuePriority.P0_CRITICAL

            LabEventType.SHOT_ON_TARGET,
            LabEventType.SHOT,
            LabEventType.HEADER,
            LabEventType.SAVE,
            LabEventType.SHOT_OFF_TARGET,
            LabEventType.REBOUND,
            LabEventType.YELLOW_CARD,
            LabEventType.INJURY,
            LabEventType.SUBSTITUTION,
            LabEventType.OFFSIDE,
            LabEventType.EXTRA_TIME_STARTED,
            LabEventType.MATCH_STARTED,
            LabEventType.KICKOFF -> CuePriority.P1_HIGH

            LabEventType.CORNER,
            LabEventType.FREE_KICK,
            LabEventType.THROUGH_BALL,
            LabEventType.CROSS,
            LabEventType.FOUL,
            LabEventType.ADVANTAGE_PLAYED,
            LabEventType.DRIBBLE_WON,
            LabEventType.TACKLE_WON,
            LabEventType.INTERCEPTION,
            LabEventType.BLOCK,
            LabEventType.CLEARANCE,
            LabEventType.GOALKEEPER_CLAIM,
            LabEventType.GOALKEEPER_PUNCH -> CuePriority.P2_MEDIUM

            else -> CuePriority.P3_AMBIENT
        }
    }
}
