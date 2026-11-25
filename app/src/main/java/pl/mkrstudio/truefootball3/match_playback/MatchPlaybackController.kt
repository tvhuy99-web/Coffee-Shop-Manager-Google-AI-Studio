package pl.mkrstudio.truefootball3.match_playback

import pl.mkrstudio.truefootball3.match_commentary.NarrationScheduler
import pl.mkrstudio.truefootball3.match_audio.LabAudioDirector

import pl.mkrstudio.truefootball3.lab.*

import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import org.json.JSONArray
import org.json.JSONObject
import java.util.ArrayDeque
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import kotlin.math.abs
import kotlin.math.roundToLong

/**
 * Unified DES v1.4 Match Playback Controller.
 *
 * Replaces the legacy dual-timer / TTS-blocked tick loops with a single monotonic simulation-clock
 * controller backed by `SystemClock.elapsedRealtime()` and zero-drift rate re-anchoring across
 * `1.0x / 1.25x / 1.5x / 2.0x / 3.0x`.
 *
 * Key responsibilities:
 * - Zero-drift re-anchor when changing playback rate (`1.0x / 1.25x / 1.5x / 2.0x / 3.0x`).
 * - Full support for all 3 [LabViewingMode]s:
 *   - `FULL_MATCH`: continuous 1:1 progression scaled by `playbackRate`.
 *   - `EXTENDED_HIGHLIGHTS` & `SHORT_HIGHLIGHTS`: real-time playback inside highlight windows +
 *     automatic `FastForwardSkip` across non-highlight gaps.
 * - Native handling of `HalfTimeBreak` / `ExtraTimeBreak`: pauses automatically for tactical/sub
 *   adjustments and resumes immediately via `session.startNextPeriod()` (never waits 15 real mins).
 * - Dual-frontier checkpointing: periodically exports and atomically persists
 *   `lastCommittedRustCheckpointJson` (excluding speculative lookahead and `elapsedRealtime()`);
 *   [onHostPaused] freezes the clock (`paused = true`) and saves atomically.
 */
class MatchPlaybackController(
    private val matchId: String,
    private val seed: Long,
    private val session: LabRustLiveSession,
    private val narrationScheduler: NarrationScheduler,
    private val audioDirector: LabAudioDirector?,
    initialViewingMode: LabViewingMode = LabViewingMode.FULL_MATCH,
    private val includeTurningPointsInShort: Boolean = true,
    private val listener: Listener,
    private val checkpointStore: MatchCheckpointStore? = null,
    private val clockProvider: () -> Long = { SystemClock.elapsedRealtime() },
    private val workerExecutor: ExecutorService = Executors.newSingleThreadExecutor()
) {
    interface Listener {
        fun onEventsProduced(
            stepObj: JSONObject,
            rawEvents: List<JSONObject>,
            filteredEvents: List<JSONObject>,
            isLookahead: Boolean
        ): List<NarrationScheduler.SimulationEvent>

        fun onClockUpdated(snapshot: DesClockSnapshot, playbackRate: Float, paused: Boolean)
        fun onRosterUpdated(homeRoster: JSONObject)
        fun onPeriodBreak(stage: DesHalfStage, period: Int, homeGoals: Int, awayGoals: Int)
        fun onFastForwardSkip(fromSimTimeMs: Long, toSimTimeMs: Long)
        fun onMatchFinished(finalResult: JSONObject)
        fun onError(error: Throwable)
    }

    private val mainHandler = Handler(Looper.getMainLooper())

    @Volatile var viewingMode: LabViewingMode = initialViewingMode
        private set

    @Volatile private var playbackRate: Float = 1.0f
    @Volatile private var paused: Boolean = true
    @Volatile private var started: Boolean = false
    @Volatile private var finished: Boolean = false
    @Volatile private var released: Boolean = false
    @Volatile private var stepInFlight: Boolean = false
    private var checkpointInFlight = false
    private var extraLookaheadMs = 0L
    private var checkpointRequested = false
    private var commandsInFlight = 0
    private var latestPresentedRoster: JSONObject? = null

    // Zero-drift clock anchor pair
    @Volatile private var anchorWallMs: Long = clockProvider()
    @Volatile private var anchorSimTimeMs: Long = 0L

    // Dual-frontier tracking: committed frontier vs simulated frontier
    @Volatile private var committedSimTimeMs: Long = 0L
    @Volatile private var simulatedSimTimeMs: Long = 0L
    @Volatile private var lastPresentedEventSeq: Int = 0
    @Volatile private var lastCommittedRustCheckpointJson: String? = null
    @Volatile private var lastCheckpointSavedSimTimeMs: Long = -CHECKPOINT_INTERVAL_SIM_MS

    // Latest authoritative match clock metrics from Rust DES
    @Volatile private var currentPeriod: Int = 1
    @Volatile private var currentPeriodElapsedMs: Long = 0L
    @Volatile private var currentBallInPlayMs: Long = 0L
    @Volatile private var currentBallOutOfPlayMs: Long = 0L
    @Volatile private var currentCompensableLostMs: Long = 0L
    @Volatile private var currentAnnouncedAddedMs: Long = 0L
    @Volatile private var currentAdditionalAddedMs: Long = 0L
    @Volatile private var currentEffectiveAddedMs: Long = 0L
    @Volatile private var currentHalfStage: DesHalfStage = DesHalfStage.PRE_MATCH
    @Volatile private var simFrontierStage: DesHalfStage = DesHalfStage.PRE_MATCH
    @Volatile private var simFrontierBreakSimTimeMs: Long? = null
    @Volatile private var simFrontierFullTimeSimTimeMs: Long? = null
    @Volatile private var homeGoals: Int = 0
    @Volatile private var awayGoals: Int = 0
    @Volatile private var presentedHomeGoals: Int = 0
    @Volatile private var presentedAwayGoals: Int = 0
    @Volatile private var presentedPeriod: Int = 1
    @Volatile private var currentPeriodStartSimTimeMs: Long = 0L
    @Volatile private var presentedAnnouncedAddedMs: Long = 0L
    private val pendingPresentationEvents = ArrayDeque<JSONObject>()
    private val pendingRosterUpdates = ArrayDeque<Pair<Long, JSONObject>>()

    // Highlight window tracking for EXTENDED_HIGHLIGHTS / SHORT_HIGHLIGHTS
    @Volatile private var activeHighlightUntilSimTimeMs: Long = -1L

    private val frameRunnable = object : Runnable {
        override fun run() {
            if (released || finished || paused) return
            if (currentHalfStage == DesHalfStage.HALF_TIME_BREAK ||
                currentHalfStage == DesHalfStage.EXTRA_TIME_BREAK ||
                currentHalfStage == DesHalfStage.FULL_TIME
            ) {
                return
            }

            val idealSimMs = computeCurrentSimTimeMs()
            val limit = minOf(simulatedSimTimeMs, simFrontierBreakSimTimeMs ?: Long.MAX_VALUE,
                simFrontierFullTimeSimTimeMs ?: Long.MAX_VALUE)
            val nowSimMs = minOf(idealSimMs, limit).coerceAtLeast(committedSimTimeMs)
            if (idealSimMs > nowSimMs) {
                anchorSimTimeMs = nowSimMs
                anchorWallMs = clockProvider()
            }
            committedSimTimeMs = nowSimMs

            // CRITICAL-02 & Supplementary #6: Drain buffered events strictly up to PresentationFrontier
            // (`nowSimMs`), triggering LabAudioDirector SFX for EVERY event (decoupled from TTS selection).
            drainPresentationEventsUpTo(nowSimMs)

            // Drive NarrationScheduler at committed simTimeMs (TTS never blocks clock!)
            narrationScheduler.onClockTick(nowSimMs)

            // Emit clock UI update strictly from PresentationFrontier (`nowSimMs`)
            listener.onClockUpdated(buildClockSnapshot(nowSimMs), playbackRate, paused)

            // CRITICAL-02: Only enter HalfTimeBreak/ExtraTimeBreak when PresentationFrontier reaches it!
            val breakAt = simFrontierBreakSimTimeMs
            if (breakAt != null && nowSimMs >= breakAt && pendingPresentationEvents.isEmpty()) {
                triggerPresentationPeriodBreak(simFrontierStage, breakAt)
                return
            }

            // CRITICAL-02: Only finish match when PresentationFrontier reaches FullTime AND all events played!
            val ftAt = simFrontierFullTimeSimTimeMs
            if (ftAt != null && nowSimMs >= ftAt && pendingPresentationEvents.isEmpty()) {
                triggerPresentationFullTime(ftAt)
                return
            }

            // Request more simulation from Rust only if SimulationFrontier has not reached a break/FT
            val simStopped = simFrontierStage == DesHalfStage.HALF_TIME_BREAK ||
                simFrontierStage == DesHalfStage.EXTRA_TIME_BREAK ||
                simFrontierStage == DesHalfStage.FULL_TIME
            val targetAheadMs = nowSimMs + narrationScheduler.lookaheadMs + extraLookaheadMs
            if (!simStopped && !stepInFlight && !checkpointInFlight && session.isOpen && simulatedSimTimeMs < targetAheadMs) {
                requestAdvanceUntil(targetAheadMs)
            }

            // Periodic dual-frontier committed checkpoint save
            if (nowSimMs - lastCheckpointSavedSimTimeMs >= CHECKPOINT_INTERVAL_SIM_MS) {
                exportAndSaveCommittedCheckpointAsync(force = false)
            }

            mainHandler.postDelayed(this, FRAME_TICK_MS)
        }
    }

    /**
     * Computes exact current simulation time in milliseconds from the zero-drift wall-clock anchor.
     */
    fun computeCurrentSimTimeMs(nowWallMs: Long = clockProvider()): Long {
        if (paused ||
            finished ||
            currentHalfStage == DesHalfStage.HALF_TIME_BREAK ||
            currentHalfStage == DesHalfStage.EXTRA_TIME_BREAK ||
            currentHalfStage == DesHalfStage.FULL_TIME
        ) {
            return anchorSimTimeMs
        }
        val wallDelta = (nowWallMs - anchorWallMs).coerceAtLeast(0L)
        return anchorSimTimeMs + (wallDelta * playbackRate).roundToLong()
    }

    fun presentedScore(): Pair<Int, Int> = presentedHomeGoals to presentedAwayGoals

    fun currentPlaybackRate(): Float = playbackRate

    fun isPaused(): Boolean = paused

    fun isFinished(): Boolean = finished

    fun currentHalfStage(): DesHalfStage = currentHalfStage

    fun committedSimTimeMs(): Long = committedSimTimeMs

    fun lastCommittedCheckpointJson(): String? = lastCommittedRustCheckpointJson

    fun recordPresentedEventSeq(seq: Int) {
        if (seq > lastPresentedEventSeq) {
            lastPresentedEventSeq = seq
        }
    }

    /**
     * Zero-drift playback rate change (`1.0x / 1.25x / 1.5x / 2.0x / 3.0x`).
     * Re-anchors `(anchorSimTimeMs, anchorWallMs)` BEFORE updating `playbackRate` so there is
     * never any backward jump or accumulated time drift.
     */
    fun setPlaybackRate(newRate: Float) {
        val clamped = normalizeRate(newRate)
        val nowWall = clockProvider()
        val exactSimNow = computeCurrentSimTimeMs(nowWall)
        anchorSimTimeMs = exactSimNow
        anchorWallMs = nowWall
        playbackRate = clamped
        narrationScheduler.setPlaybackRate(clamped)
        LabLogStore.log(
            "PLAYBACK",
            "Re-anchored playbackRate -> ${clamped}x at simTimeMs=$exactSimNow"
        )
    }

    fun setViewingMode(mode: LabViewingMode) {
        viewingMode = mode
        LabLogStore.log("PLAYBACK", "ViewingMode -> $mode")
    }

    fun start() {
        if (released || finished) return
        started = true
        paused = false
        anchorWallMs = clockProvider()
        if (currentHalfStage == DesHalfStage.PRE_MATCH) {
            currentHalfStage = DesHalfStage.IN_HALF
        }
        mainHandler.removeCallbacks(frameRunnable)
        mainHandler.post(frameRunnable)
    }

    fun pause() {
        if (paused) return
        val nowWall = clockProvider()
        anchorSimTimeMs = minOf(computeCurrentSimTimeMs(nowWall), simulatedSimTimeMs,
            simFrontierBreakSimTimeMs ?: Long.MAX_VALUE, simFrontierFullTimeSimTimeMs ?: Long.MAX_VALUE).coerceAtLeast(committedSimTimeMs)
        anchorWallMs = nowWall
        committedSimTimeMs = anchorSimTimeMs
        paused = true
        mainHandler.removeCallbacks(frameRunnable)
        narrationScheduler.pause()
        LabLogStore.log("PLAYBACK", "Paused at simTimeMs=$anchorSimTimeMs")
    }

    fun resume() {
        if (released || finished) return
        if (currentHalfStage == DesHalfStage.HALF_TIME_BREAK ||
            currentHalfStage == DesHalfStage.EXTRA_TIME_BREAK
        ) {
            continueFromPeriodBreak()
            return
        }
        if (!paused) return
        // Re-anchor wall clock WITHOUT adding paused wall-clock interval
        anchorWallMs = clockProvider()
        paused = false
        narrationScheduler.resume()
        mainHandler.removeCallbacks(frameRunnable)
        mainHandler.post(frameRunnable)
        LabLogStore.log("PLAYBACK", "Resumed from simTimeMs=$anchorSimTimeMs")
    }

    /**
     * Called from Activity `onPause()` / `onStop()`.
     * Freezes the clock immediately (`paused = true`), stops lookahead drift, and atomically
     * persists the committed frontier checkpoint (never speculative lookahead or elapsedRealtime).
     */
    fun onHostPaused() {
        if (released || finished) return
        pause()
        exportAndSaveCommittedCheckpointAsync(force = true)
    }

    /**
     * Restores playback controller state from a verified [PlaybackCheckpointState].
     * Per DES v1.4 contract, ALWAYS resumes in `paused = true` state with zero added wall-clock time.
     */
    fun restoreFromCheckpoint(state: PlaybackCheckpointState) {
        val presentation = JSONObject(state.presentationStateJson)
        presentedHomeGoals = presentation.getInt("homeGoals")
        presentedAwayGoals = presentation.getInt("awayGoals")
        presentedPeriod = presentation.getInt("period")
        currentPeriodStartSimTimeMs = presentation.getLong("periodStartMs")
        presentedAnnouncedAddedMs = presentation.getLong("announcedAddedMs")
        currentHalfStage = DesHalfStage.valueOf(presentation.getString("stage"))
        simFrontierStage = DesHalfStage.valueOf(presentation.getString("frontierStage"))
        simFrontierBreakSimTimeMs = presentation.optLong("breakAt", -1L).takeIf { it >= 0L }
        simFrontierFullTimeSimTimeMs = presentation.optLong("fullTimeAt", -1L).takeIf { it >= 0L }
        currentPeriod = presentation.optInt("nativePeriod", presentedPeriod)
        latestPresentedRoster = presentation.optJSONObject("roster")
        latestPresentedRoster?.let(listener::onRosterUpdated)
        pendingRosterUpdates.clear()
        val rosterUpdates = presentation.optJSONArray("rosterUpdates") ?: JSONArray()
        for (i in 0 until rosterUpdates.length()) {
            val update = rosterUpdates.getJSONObject(i)
            pendingRosterUpdates.addLast(update.getLong("time") to update.getJSONObject("roster"))
        }
        restoreBufferedUnpresentedEventsJson(state.unpresentedEventsJson)
        narrationScheduler.pause()
        narrationScheduler.restorePendingCuesJson(
            cuesJson = state.pendingCuesJson,
            inProgressCueId = state.inProgressCueId,
            inProgressOffsetMs = state.inProgressCueOffsetMs
        )
        paused = true
        started = true
        anchorSimTimeMs = state.playbackSimTimeMs.coerceAtLeast(0L)
        committedSimTimeMs = anchorSimTimeMs
        simulatedSimTimeMs = state.committedRustSimTimeMs.coerceAtLeast(anchorSimTimeMs)
        anchorWallMs = clockProvider()
        lastPresentedEventSeq = state.lastPresentedEventSeq
        playbackRate = normalizeRate(state.playbackRate)
        viewingMode = try {
            LabViewingMode.valueOf(state.viewingMode)
        } catch (_: IllegalArgumentException) {
            LabViewingMode.FULL_MATCH
        }
        narrationScheduler.setPlaybackRate(playbackRate)
        narrationScheduler.pause()
        listener.onClockUpdated(buildClockSnapshot(anchorSimTimeMs), playbackRate, true)
        if (currentHalfStage == DesHalfStage.HALF_TIME_BREAK || currentHalfStage == DesHalfStage.EXTRA_TIME_BREAK) {
            listener.onPeriodBreak(currentHalfStage, presentedPeriod, presentedHomeGoals, presentedAwayGoals)
        }
        val restoredCrowd = try {
            LabAudioDirector.CrowdState.valueOf(state.crowdState)
        } catch (_: IllegalArgumentException) {
            LabAudioDirector.CrowdState.CALM
        }
        audioDirector?.restoreState(
            LabAudioDirector.DirectorState(
                crowdState = restoredCrowd,
                lastTransitionSimTimeMs = state.lastTransitionSimTimeMs,
                lastCrowdRiseSimTimeMs = state.lastCrowdRiseSimTimeMs
            )
        )
        LabLogStore.log(
            "PLAYBACK",
            "Restored checkpoint matchId=$matchId simTimeMs=$anchorSimTimeMs paused=true"
        )
    }

    /**
     * Continues match from `HalfTimeBreak` or `ExtraTimeBreak` by calling Rust `startNextPeriod()`,
     * revoking stale lookahead, re-anchoring clock, and resuming immediately without waiting 15 min.
     */
    fun continueFromPeriodBreak() {
        if (released || finished || !session.isOpen) return
        if (stepInFlight || checkpointInFlight) return
        stepInFlight = true

        workerExecutor.execute {
            try {
                val raw = session.startNextPeriod()
                val stepObj = JSONObject(raw)
                mainHandler.post {
                    stepInFlight = false
                    if (released) return@post
                    if (!stepObj.optBoolean("ok", false)) {
                        listener.onError(IllegalStateException(stepObj.optString("error", "startNextPeriod failed")))
                        return@post
                    }
                    processStepResultOnMain(stepObj, isFromPeriodStart = true)
                    if (checkpointRequested) exportAndSaveCommittedCheckpointAsync(force = true)
                    if (!finished && currentHalfStage == DesHalfStage.IN_HALF) {
                        anchorWallMs = clockProvider()
                        paused = false
                        narrationScheduler.resume()
                        mainHandler.removeCallbacks(frameRunnable)
                        mainHandler.post(frameRunnable)
                    }
                }
            } catch (error: Exception) {
                mainHandler.post {
                    stepInFlight = false
                    if (!released) listener.onError(error)
                }
            }
        }
    }

    /**
     * Submits a live tactical or substitution command JSON to Rust DES and invalidates speculative
     * lookahead cues beyond the committed frontier so future narration reflects the new decision.
     */
    fun submitCommandJson(commandJson: String, onResult: (ok: Boolean, responseJson: String) -> Unit) {
        if (released || finished || !session.isOpen) {
            onResult(false, "{\"ok\":false,\"error\":\"session_not_active\"}")
            return
        }
        commandsInFlight++
        workerExecutor.execute {
            try {
                val resp = session.submitCommandJson(commandJson)
                val parsed = try { JSONObject(resp) } catch (_: Exception) { null }
                val ok = parsed?.optBoolean("ok", true) ?: true
                mainHandler.post {
                    commandsInFlight--
                    if (!released) {
                        // Native commands are accepted at its confirmed frontier. Never revoke
                        // already produced football events without a corresponding rollback.
                        if (ok) exportAndSaveCommittedCheckpointAsync(force = true)
                        onResult(ok, resp)
                    }
                }
            } catch (error: Exception) {
                mainHandler.post {
                    commandsInFlight--
                    if (!released) onResult(false, "{\"ok\":false,\"error\":\"${error.message}\"}")
                }
            }
        }
    }

    fun release() {
        released = true
        paused = true
        mainHandler.removeCallbacks(frameRunnable)
        narrationScheduler.release()
    }

    private fun requestAdvanceUntil(targetSimTimeMs: Long) {
        if (stepInFlight || released || finished || !session.isOpen) return
        stepInFlight = true

        workerExecutor.execute {
            try {
                val raw = session.advanceUntilMs(targetSimTimeMs, MAX_STEPS_GUARD)
                val stepObj = JSONObject(raw)
                mainHandler.post {
                    stepInFlight = false
                    if (released) return@post
                    if (!stepObj.optBoolean("ok", false)) {
                        listener.onError(IllegalStateException(stepObj.optString("error", "advanceUntilMs failed")))
                        return@post
                    }
                    val previousFrontier = simulatedSimTimeMs
                    processStepResultOnMain(stepObj, isFromPeriodStart = false)
                    // DES may have no event inside the requested window (injury, goal celebration).
                    // Grow the request rather than deadlocking on an unchanged frontier.
                    extraLookaheadMs = if (simulatedSimTimeMs > previousFrontier) 0L
                        else (extraLookaheadMs + narrationScheduler.lookaheadMs).coerceAtMost(300_000L)
                    if (checkpointRequested) exportAndSaveCommittedCheckpointAsync(force = true)
                }
            } catch (error: Exception) {
                mainHandler.post {
                    stepInFlight = false
                    if (!released) listener.onError(error)
                }
            }
        }
    }

    private fun processStepResultOnMain(stepObj: JSONObject, isFromPeriodStart: Boolean) {
        val stepSimTimeMs = stepObj.optLong("simTimeMs", stepObj.optInt("gameSecond", 0) * 1000L)
        simulatedSimTimeMs = maxOf(simulatedSimTimeMs, stepSimTimeMs)
        currentPeriod = stepObj.optInt("period", currentPeriod)
        currentPeriodElapsedMs = stepObj.optLong(
            "periodElapsedMs",
            stepObj.optInt("periodElapsedSecond", 0) * 1000L
        )
        currentBallInPlayMs = stepObj.optLong("ballInPlayMs", currentBallInPlayMs)
        currentBallOutOfPlayMs = stepObj.optLong("ballOutOfPlayMs", currentBallOutOfPlayMs)
        currentCompensableLostMs = stepObj.optLong("compensableLostMs", currentCompensableLostMs)
        currentAnnouncedAddedMs = stepObj.optLong("announcedAddedMs", currentAnnouncedAddedMs)
        currentAdditionalAddedMs = stepObj.optLong("additionalAddedMs", currentAdditionalAddedMs)
        currentEffectiveAddedMs = stepObj.optLong("effectiveAddedMs", currentEffectiveAddedMs)
        homeGoals = stepObj.optInt("homeGoals", homeGoals)
        awayGoals = stepObj.optInt("awayGoals", awayGoals)

        val stage = if (stepObj.optBoolean("isFullTime", false)) {
            DesHalfStage.FULL_TIME
        } else {
            DesHalfStage.fromRustStage(stepObj.optString("stage", "IN_HALF"))
        }
        simFrontierStage = stage
        if (currentHalfStage == DesHalfStage.PRE_MATCH || isFromPeriodStart) {
            currentHalfStage = DesHalfStage.IN_HALF
            simFrontierBreakSimTimeMs = null
        }

        stepObj.optJSONObject("homeRoster")?.let { roster ->
            if (isFromPeriodStart || stepSimTimeMs <= committedSimTimeMs) {
                latestPresentedRoster = roster
                listener.onRosterUpdated(roster)
            } else {
                pendingRosterUpdates.addLast(stepSimTimeMs to roster)
            }
        }

        val rawArr = stepObj.optJSONArray("newEvents") ?: JSONArray()
        val rawList = ArrayList<JSONObject>(rawArr.length())
        for (i in 0 until rawArr.length()) {
            val evObj = rawArr.getJSONObject(i)
            rawList.add(evObj)
            val evSeq = evObj.optInt("sequence", -1)
            if (evSeq > lastPresentedEventSeq) {
                pendingPresentationEvents.addLast(evObj)
            }
        }

        val filteredList = LabV4PlaybackCoordinator.filterEventsForViewingMode(
            rawArr,
            viewingMode,
            includeTurningPointsInShort
        )

        val simEvents = listener.onEventsProduced(
            stepObj = stepObj,
            rawEvents = rawList,
            filteredEvents = filteredList,
            isLookahead = !isFromPeriodStart && stepSimTimeMs > committedSimTimeMs
        )

        if (simEvents.isNotEmpty()) {
            narrationScheduler.ingestSimulationEvents(simEvents, isSpeculative = !isFromPeriodStart)
        }

        // Highlights mode: fast-forward skip across non-highlight gaps while playing highlight windows at 1:1 rate
        if (viewingMode != LabViewingMode.FULL_MATCH && stage == DesHalfStage.IN_HALF) {
            if (filteredList.isNotEmpty()) {
                val firstHighlightMs = filteredList.first().let {
                    it.optLong("simTimeMs", it.optInt("gameSecond", 0) * 1000L)
                }
                val lastHighlightMs = filteredList.last().let {
                    it.optLong("simTimeMs", it.optInt("gameSecond", 0) * 1000L)
                }
                activeHighlightUntilSimTimeMs = maxOf(
                    activeHighlightUntilSimTimeMs,
                    lastHighlightMs + HIGHLIGHT_WINDOW_TAIL_MS
                )
                if (firstHighlightMs - committedSimTimeMs > HIGHLIGHT_SKIP_GAP_MS) {
                    val skipTargetMs = (firstHighlightMs - HIGHLIGHT_LEAD_IN_MS).coerceAtLeast(committedSimTimeMs)
                    performFastForwardSkip(committedSimTimeMs, skipTargetMs)
                }
            } else if (committedSimTimeMs > activeHighlightUntilSimTimeMs &&
                stepSimTimeMs - committedSimTimeMs >= HIGHLIGHT_SKIP_GAP_MS
            ) {
                performFastForwardSkip(committedSimTimeMs, stepSimTimeMs)
            }
        }

        if (isFromPeriodStart) {
            anchorSimTimeMs = stepSimTimeMs
            committedSimTimeMs = stepSimTimeMs
            anchorWallMs = clockProvider()
        }

        // CRITICAL-02: Never cut off the user's 90-minute viewing clock when Rust lookahead hits
        // HalfTimeBreak/ExtraTimeBreak/FullTime early. Record the frontier timestamp and let
        // `frameRunnable` present every remaining second & event up to that exact timestamp!
        if (stage == DesHalfStage.HALF_TIME_BREAK || stage == DesHalfStage.EXTRA_TIME_BREAK) {
            simFrontierBreakSimTimeMs = stepSimTimeMs
            if (committedSimTimeMs >= stepSimTimeMs && pendingPresentationEvents.isEmpty()) {
                triggerPresentationPeriodBreak(stage, stepSimTimeMs)
            }
            return
        }

        if (stage == DesHalfStage.FULL_TIME || stepObj.optBoolean("isFullTime", false)) {
            simFrontierFullTimeSimTimeMs = stepSimTimeMs
            if (committedSimTimeMs >= stepSimTimeMs && pendingPresentationEvents.isEmpty()) {
                triggerPresentationFullTime(stepSimTimeMs)
            }
        }
    }

    private fun drainPresentationEventsUpTo(nowSimMs: Long) {
        while (pendingRosterUpdates.isNotEmpty() && pendingRosterUpdates.first().first <= nowSimMs) {
            val roster = pendingRosterUpdates.removeFirst().second
            latestPresentedRoster = roster
            listener.onRosterUpdated(roster)
        }
        while (pendingPresentationEvents.isNotEmpty()) {
            val head = pendingPresentationEvents.first()
            val evSimMs = head.optLong("simTimeMs", head.optInt("gameSecond", 0) * 1000L)
            if (evSimMs > nowSimMs) break
            pendingPresentationEvents.removeFirst()
            val seq = head.optInt("sequence", lastPresentedEventSeq + 1)
            if (seq > lastPresentedEventSeq) lastPresentedEventSeq = seq
            val kindStr = head.optString("kind", "")
            val sideStr = head.optString("side", "")
            if (kindStr == "GOAL" || kindStr == "OWN_GOAL") {
                if (sideStr == "HOME") presentedHomeGoals++
                else if (sideStr == "AWAY") presentedAwayGoals++
            }
            // Supplementary #6: Dispatch SFX for EVERY presented event independent of TTS cue selection.
            val labKind = try { LabEventType.valueOf(kindStr) } catch (_: Exception) { null }
            val labSide = when (sideStr) {
                "HOME" -> LabTeamSide.HOME
                "AWAY" -> LabTeamSide.AWAY
                else -> null
            }
            if (kindStr == "ADDED_TIME_ANNOUNCED") {
                val meta = head.optJSONObject("metadata")
                val addedMs = meta?.optJSONObject("extra")?.optString("addedMs")?.toLongOrNull()
                    ?: meta?.optLong("addedMs", -1L) ?: -1L
                if (addedMs >= 0L) {
                    presentedAnnouncedAddedMs = addedMs
                }
            }
            val evPeriod = head.optInt("period", presentedPeriod)
            if (evPeriod != presentedPeriod && evPeriod in 1..4) {
                presentedPeriod = evPeriod
                currentPeriodStartSimTimeMs = evSimMs - head.optLong("periodElapsedMs", 0L)
                presentedAnnouncedAddedMs = 0L
            }
            if (labKind != null) {
                val evMinute = ((evSimMs / 60_000L).toInt() + 1).coerceAtLeast(1)
                val goalDiffAbs = kotlin.math.abs(presentedHomeGoals - presentedAwayGoals)
                audioDirector?.onEvent(
                    kind = labKind,
                    side = labSide,
                    minute = evMinute,
                    goalDiffAbs = goalDiffAbs,
                    simTimeMs = evSimMs,
                    isResumedReplay = false
                )
            }
        }
    }

    private fun triggerPresentationPeriodBreak(stage: DesHalfStage, breakSimTimeMs: Long) {
        currentHalfStage = stage
        anchorSimTimeMs = breakSimTimeMs
        committedSimTimeMs = breakSimTimeMs
        anchorWallMs = clockProvider()
        paused = true
        mainHandler.removeCallbacks(frameRunnable)
        exportAndSaveCommittedCheckpointAsync(force = true)
        narrationScheduler.reportBoundaryBacklog(breakSimTimeMs)
        listener.onPeriodBreak(stage, presentedPeriod, presentedHomeGoals, presentedAwayGoals)
    }

    private fun triggerPresentationFullTime(fullTimeSimTimeMs: Long) {
        currentHalfStage = DesHalfStage.FULL_TIME
        finished = true
        paused = true
        anchorSimTimeMs = fullTimeSimTimeMs
        committedSimTimeMs = fullTimeSimTimeMs
        mainHandler.removeCallbacks(frameRunnable)
        narrationScheduler.onClockTick(fullTimeSimTimeMs)
        narrationScheduler.reportBoundaryBacklog(fullTimeSimTimeMs)
        try {
            checkpointStore?.deleteCheckpoint(matchId)
        } catch (_: Exception) {
        }
        workerExecutor.execute {
            try {
                val finalObj = session.finish()
                session.close()
                mainHandler.post {
                    if (!released) listener.onMatchFinished(finalObj)
                }
            } catch (error: Exception) {
                mainHandler.post {
                    if (!released) listener.onError(error)
                }
            }
        }
    }

    fun exportBufferedUnpresentedEventsJson(): String {
        val arr = JSONArray()
        for (ev in pendingPresentationEvents) {
            arr.put(ev)
        }
        return arr.toString()
    }

    fun restoreBufferedUnpresentedEventsJson(json: String) {
        pendingPresentationEvents.clear()
        if (json.isBlank() || json == "[]") return
        try {
            val arr = JSONArray(json)
            for (i in 0 until arr.length()) {
                pendingPresentationEvents.addLast(arr.getJSONObject(i))
            }
        } catch (_: Exception) {
        }
    }

    private fun performFastForwardSkip(fromSimMs: Long, toSimMs: Long) {
        if (toSimMs <= fromSimMs) return
        anchorSimTimeMs = toSimMs
        committedSimTimeMs = toSimMs
        anchorWallMs = clockProvider()
        audioDirector?.onFastForwardSkip(toSimMs)
        narrationScheduler.onFastForwardSkip(toSimMs)
        listener.onFastForwardSkip(fromSimMs, toSimMs)
        LabLogStore.log("PLAYBACK", "FastForwardSkip ($viewingMode): $fromSimMs -> $toSimMs ms")
    }

    private fun exportAndSaveCommittedCheckpointAsync(force: Boolean) {
        val store = checkpointStore ?: return
        if (!session.isOpen || finished) return
        if (checkpointInFlight || stepInFlight || commandsInFlight > 0) {
            checkpointRequested = checkpointRequested || force
            return
        }
        val snapshotSimMs = committedSimTimeMs
        if (!force && snapshotSimMs - lastCheckpointSavedSimTimeMs < CHECKPOINT_INTERVAL_SIM_MS) return
        checkpointRequested = false
        checkpointInFlight = true
        lastCheckpointSavedSimTimeMs = snapshotSimMs

        val cueInfo = narrationScheduler.exportCheckpointCueInfo()
        val directorState = audioDirector?.exportState() ?: LabAudioDirector.DirectorState()
        val playbackState = PlaybackCheckpointState(
            playbackSimTimeMs = snapshotSimMs,
            committedRustSimTimeMs = simulatedSimTimeMs,
            playbackRate = playbackRate,
            paused = true,
            viewingMode = viewingMode.name,
            includeTurningPointsInShort = includeTurningPointsInShort,
            lastPresentedEventSeq = lastPresentedEventSeq,
            lastCompletedCueId = cueInfo.lastCompletedCueId,
            inProgressCueId = cueInfo.inProgressCueId,
            inProgressCueAnchorSimMs = cueInfo.inProgressCueAnchorSimMs,
            inProgressCueText = cueInfo.inProgressCueText,
            inProgressCueOffsetMs = cueInfo.inProgressCueOffsetMs,
            crowdState = directorState.crowdState.name,
            lastTransitionSimTimeMs = directorState.lastTransitionSimTimeMs,
            lastCrowdRiseSimTimeMs = directorState.lastCrowdRiseSimTimeMs,
            unpresentedEventsJson = exportBufferedUnpresentedEventsJson(),
            pendingCuesJson = narrationScheduler.exportPendingCuesJson(),
            presentationStateJson = JSONObject()
                .put("homeGoals", presentedHomeGoals).put("awayGoals", presentedAwayGoals)
                .put("period", presentedPeriod).put("periodStartMs", currentPeriodStartSimTimeMs)
                .put("announcedAddedMs", presentedAnnouncedAddedMs)
                .put("stage", currentHalfStage.name).put("frontierStage", simFrontierStage.name)
                .put("nativePeriod", currentPeriod)
                .put("breakAt", simFrontierBreakSimTimeMs ?: -1L)
                .put("fullTimeAt", simFrontierFullTimeSimTimeMs ?: -1L)
                .put("roster", latestPresentedRoster ?: JSONObject.NULL)
                .put("rosterUpdates", JSONArray().also { arr -> pendingRosterUpdates.forEach { (time, roster) ->
                    arr.put(JSONObject().put("time", time).put("roster", roster))
                } }).toString()
        )

        workerExecutor.execute {
            try {
                if (!session.isOpen) return@execute
                val rustJson = session.exportCheckpointJson()
                if (rustJson.isNotBlank() && !rustJson.contains("\"ok\":false")) {
                    lastCommittedRustCheckpointJson = rustJson
                    store.saveAtomic(
                        MatchCheckpointStore.MatchCheckpointEnvelope(
                            matchId = matchId,
                            seed = seed,
                            rustCheckpointJson = rustJson,
                            androidPlayback = playbackState
                        )
                    )
                }
            } catch (e: Exception) {
                LabLogStore.error("CHECKPOINT", "Dual-frontier checkpoint save failed", e)
            } finally {
                mainHandler.post {
                    checkpointInFlight = false
                    if (checkpointRequested && !released) exportAndSaveCommittedCheckpointAsync(force = true)
                }
            }
        }
    }

    private fun buildClockSnapshot(simTimeMs: Long): DesClockSnapshot {
        val presElapsedMs = (simTimeMs - currentPeriodStartSimTimeMs).coerceAtLeast(0L)
        val clampedBallInPlay = currentBallInPlayMs.coerceAtMost(simTimeMs)
        val clampedBallOut = (simTimeMs - clampedBallInPlay).coerceAtLeast(0L)
        return DesClockSnapshot(
            simTimeMs = simTimeMs,
            period = presentedPeriod,
            periodElapsedMs = presElapsedMs,
            ballInPlayMs = clampedBallInPlay,
            ballOutOfPlayMs = clampedBallOut,
            compensableLostMs = currentCompensableLostMs.coerceAtMost(clampedBallOut),
            announcedAddedMs = presentedAnnouncedAddedMs,
            additionalAddedMs = if (presentedAnnouncedAddedMs > 0L) currentAdditionalAddedMs else 0L,
            effectiveAddedMs = if (presentedAnnouncedAddedMs > 0L) {
                presentedAnnouncedAddedMs + currentAdditionalAddedMs
            } else {
                0L
            }
        )
    }

    companion object {
        private const val FRAME_TICK_MS = 100L
        private const val MAX_STEPS_GUARD = 500
        private const val CHECKPOINT_INTERVAL_SIM_MS = 30_000L
        private const val HIGHLIGHT_SKIP_GAP_MS = 12_000L
        private const val HIGHLIGHT_LEAD_IN_MS = 2_000L
        private const val HIGHLIGHT_WINDOW_TAIL_MS = 4_000L

        fun normalizeRate(rate: Float): Float {
            val clamped = rate.coerceIn(1.0f, 3.0f)
            return NarrationScheduler.SUPPORTED_PLAYBACK_RATES.minByOrNull { abs(it - clamped) }
                ?: clamped
        }
    }
}
