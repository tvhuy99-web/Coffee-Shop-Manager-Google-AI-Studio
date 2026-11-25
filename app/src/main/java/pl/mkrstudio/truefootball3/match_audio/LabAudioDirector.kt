package pl.mkrstudio.truefootball3.match_audio

import pl.mkrstudio.truefootball3.lab.*

import org.json.JSONObject
import pl.mkrstudio.truefootball3.audio.MatchAudioManager

/**
 * DES v1.4 simulation-time-anchored audio director.
 *
 * All hysteresis, crowd decay, crowd-rise cooldown, and per-event SFX cooldown timers operate
 * strictly on `simTimeMs: Long` (never wall-clock uptime), guaranteeing
 * playback-rate invariance across 1.0x–3.0x and deterministic checkpoint save/restore.
 */
class LabAudioDirector(
    private val audio: MatchAudioManager,
    private val neutralGround: Boolean = false,
    stadiumCapacity: Int = 35000
) {
    enum class CrowdState(val rank: Int, val ambienceVolume: Float) {
        CALM(0, 0.58f),
        BUILDING(1, 0.66f),
        ATTACK(2, 0.74f),
        DANGEROUS_ATTACK(3, 0.82f),
        LATE_TENSION(3, 0.82f),
        BIG_CHANCE(4, 0.90f),
        RESOLUTION(5, 0.74f)
    }

    data class DirectorState(
        val crowdState: CrowdState = CrowdState.CALM,
        val lastTransitionSimTimeMs: Long = 0L,
        val lastCrowdRiseSimTimeMs: Long = 0L,
        val lastCueSimTimeMs: Map<LabEventType, Long> = emptyMap()
    ) {
        fun toJson(): JSONObject {
            val cues = JSONObject()
            for ((kind, ts) in lastCueSimTimeMs) {
                cues.put(kind.name, ts)
            }
            return JSONObject()
                .put("crowdState", crowdState.name)
                .put("lastTransitionSimTimeMs", lastTransitionSimTimeMs)
                .put("lastCrowdRiseSimTimeMs", lastCrowdRiseSimTimeMs)
                .put("lastCueSimTimeMs", cues)
        }

        companion object {
            fun fromJson(obj: JSONObject?): DirectorState {
                if (obj == null) return DirectorState()
                val state = try {
                    CrowdState.valueOf(obj.optString("crowdState", CrowdState.CALM.name))
                } catch (_: IllegalArgumentException) {
                    CrowdState.CALM
                }
                val cueMap = HashMap<LabEventType, Long>()
                val cuesObj = obj.optJSONObject("lastCueSimTimeMs")
                if (cuesObj != null) {
                    val keys = cuesObj.keys()
                    while (keys.hasNext()) {
                        val key = keys.next()
                        val kind = try {
                            LabEventType.valueOf(key)
                        } catch (_: IllegalArgumentException) {
                            null
                        }
                        if (kind != null) {
                            cueMap[kind] = cuesObj.optLong(key, 0L)
                        }
                    }
                }
                return DirectorState(
                    crowdState = state,
                    lastTransitionSimTimeMs = obj.optLong("lastTransitionSimTimeMs", 0L),
                    lastCrowdRiseSimTimeMs = obj.optLong("lastCrowdRiseSimTimeMs", 0L),
                    lastCueSimTimeMs = cueMap
                )
            }
        }
    }

    private var crowdState: CrowdState = CrowdState.CALM
    private var lastTransitionSimTimeMs: Long = 0L
    private var lastCrowdRiseSimTimeMs: Long = 0L
    private val lastCueSimTimeMs = HashMap<LabEventType, Long>()

    init {
        audio.setStadiumCapacity(stadiumCapacity)
    }

    companion object {
        const val HYSTERESIS_MS = 2500L
        const val DECAY_MS = 6500L
        const val CROWD_RISE_COOLDOWN_MS = 3200L
    }

    fun currentCrowdState(): CrowdState = crowdState

    fun lastTransitionSimTimeMs(): Long = lastTransitionSimTimeMs

    fun lastCrowdRiseSimTimeMs(): Long = lastCrowdRiseSimTimeMs

    fun exportState(): DirectorState = DirectorState(
        crowdState = crowdState,
        lastTransitionSimTimeMs = lastTransitionSimTimeMs,
        lastCrowdRiseSimTimeMs = lastCrowdRiseSimTimeMs,
        lastCueSimTimeMs = HashMap(lastCueSimTimeMs)
    )

    fun exportStateJson(): String = exportState().toJson().toString()

    fun restoreState(state: DirectorState) {
        crowdState = state.crowdState
        lastTransitionSimTimeMs = state.lastTransitionSimTimeMs
        lastCrowdRiseSimTimeMs = state.lastCrowdRiseSimTimeMs
        lastCueSimTimeMs.clear()
        lastCueSimTimeMs.putAll(state.lastCueSimTimeMs)
        audio.setAmbienceVolume(crowdState.ambienceVolume)
        LabLogStore.log(
            "AUDIO",
            "Restored LabAudioDirector state=$crowdState lastTransitionSimTimeMs=$lastTransitionSimTimeMs"
        )
    }

    fun restoreStateJson(json: String?) {
        if (json.isNullOrBlank()) return
        try {
            restoreState(DirectorState.fromJson(JSONObject(json)))
        } catch (e: Exception) {
            LabLogStore.error("AUDIO", "Failed to restore LabAudioDirector state JSON", e)
        }
    }

    fun onFastForwardSkip(newSimTimeMs: Long) {
        val delta = newSimTimeMs - lastTransitionSimTimeMs
        if (delta > DECAY_MS * 2L || delta < 0L) {
            if (crowdState != CrowdState.CALM) {
                crowdState = CrowdState.CALM
                audio.setAmbienceVolume(crowdState.ambienceVolume)
                LabLogStore.log("AUDIO", "Highlights skip reset CrowdState -> CALM at simTimeMs=$newSimTimeMs")
            }
            lastTransitionSimTimeMs = newSimTimeMs
        }
    }

    fun onEvent(
        kind: LabEventType,
        side: LabTeamSide?,
        minute: Int,
        goalDiffAbs: Int,
        simTimeMs: Long,
        isWoodwork: Boolean = false,
        isPenalty: Boolean = false,
        isShootoutScored: Boolean? = null,
        isResumedReplay: Boolean = false
    ) {
        val safeSimTimeMs = simTimeMs.coerceAtLeast(0L)
        decayCrowd(safeSimTimeMs)

        val targetState = when (kind) {
            LabEventType.GOAL, LabEventType.OWN_GOAL,
            LabEventType.SHOT_OFF_TARGET, LabEventType.SAVE,
            LabEventType.CLEARANCE -> CrowdState.RESOLUTION
            LabEventType.SHOT, LabEventType.SHOT_ON_TARGET,
            LabEventType.HEADER, LabEventType.PENALTY_AWARDED,
            LabEventType.PENALTY_KICK, LabEventType.PENALTY_SHOOTOUT_KICK -> CrowdState.BIG_CHANCE
            LabEventType.CROSS, LabEventType.CORNER,
            LabEventType.THROUGH_BALL, LabEventType.DRIBBLE_WON,
            LabEventType.FREE_KICK -> CrowdState.DANGEROUS_ATTACK
            LabEventType.TRANSITION_STARTED, LabEventType.PASS_COMPLETED -> {
                if (minute >= 80 && goalDiffAbs <= 1) CrowdState.LATE_TENSION else CrowdState.BUILDING
            }
            else -> crowdState
        }

        val escalate = targetState.rank > crowdState.rank || targetState == CrowdState.RESOLUTION
        if (targetState != crowdState &&
            (escalate || safeSimTimeMs - lastTransitionSimTimeMs >= HYSTERESIS_MS)
        ) {
            val prevRank = crowdState.rank
            crowdState = targetState
            lastTransitionSimTimeMs = safeSimTimeMs
            audio.setAmbienceVolume(crowdState.ambienceVolume)
            LabLogStore.log(
                "AUDIO",
                "CrowdState -> $crowdState (ambience=${crowdState.ambienceVolume}, simTimeMs=$safeSimTimeMs)"
            )
            if (!isResumedReplay &&
                targetState.rank >= CrowdState.DANGEROUS_ATTACK.rank &&
                targetState != CrowdState.RESOLUTION &&
                prevRank < targetState.rank &&
                (lastCrowdRiseSimTimeMs == 0L || safeSimTimeMs - lastCrowdRiseSimTimeMs >= CROWD_RISE_COOLDOWN_MS)
            ) {
                LabLogStore.log("AUDIO", "Play Cue.CROWD_RISE (min=$minute, simTimeMs=$safeSimTimeMs)")
                audio.play(MatchAudioManager.Cue.CROWD_RISE)
                lastCrowdRiseSimTimeMs = safeSimTimeMs
            }
        }

        // Checkpoint/resumed replay updates crowd ambience state idempotently without re-firing one-shot SFX (e.g. goal horns).
        if (isResumedReplay) {
            lastCueSimTimeMs[kind] = safeSimTimeMs
            return
        }

        if (!allowCooldown(kind, safeSimTimeMs)) return

        when (kind) {
            LabEventType.MATCH_STARTED, LabEventType.KICKOFF -> {
                audio.playKickOff()
                audio.setAmbienceVolume(CrowdState.CALM.ambienceVolume)
            }
            LabEventType.GOAL, LabEventType.OWN_GOAL -> {
                val isHome = neutralGround || side != LabTeamSide.AWAY
                audio.duckAmbience(55)
                if (isPenalty) {
                    audio.playPenaltyGoal(isHome)
                } else if (isHome) {
                    audio.playHomeGoal()
                } else {
                    audio.playAwayGoal()
                }
            }
            LabEventType.PENALTY_SHOOTOUT_KICK -> {
                val isHome = neutralGround || side != LabTeamSide.AWAY
                if (isShootoutScored == true) {
                    audio.playPenaltyGoal(isHome)
                } else {
                    audio.playPenaltyMiss()
                }
            }
            LabEventType.PASS_COMPLETED, LabEventType.THROUGH_BALL,
            LabEventType.CROSS, LabEventType.CORNER,
            LabEventType.FREE_KICK, LabEventType.GOAL_KICK -> {
                audio.play(MatchAudioManager.Cue.PASS_KICK)
            }
            LabEventType.SHOT, LabEventType.SHOT_ON_TARGET, LabEventType.PENALTY_KICK -> {
                audio.play(MatchAudioManager.Cue.STRONG_SHOT)
            }
            LabEventType.HEADER -> audio.play(MatchAudioManager.Cue.HEADER)
            LabEventType.SAVE, LabEventType.GOALKEEPER_CLAIM, LabEventType.GOALKEEPER_PUNCH -> {
                audio.play(MatchAudioManager.Cue.KEEPER_SAVE)
            }
            LabEventType.SHOT_OFF_TARGET -> {
                when {
                    isWoodwork -> {
                        LabLogStore.log("AUDIO", "Play Cue.POST_CROSSBAR (min=$minute, simTimeMs=$safeSimTimeMs)")
                        audio.play(MatchAudioManager.Cue.POST_CROSSBAR)
                    }
                    isPenalty -> audio.playPenaltyMiss()
                    else -> audio.play(MatchAudioManager.Cue.NEAR_MISS)
                }
            }
            LabEventType.REBOUND -> {
                if (isWoodwork) audio.play(MatchAudioManager.Cue.POST_CROSSBAR)
            }
            LabEventType.FOUL, LabEventType.OFFSIDE,
            LabEventType.PENALTY_AWARDED, LabEventType.YELLOW_CARD -> {
                audio.play(MatchAudioManager.Cue.WHISTLE)
            }
            LabEventType.RED_CARD -> {
                audio.play(MatchAudioManager.Cue.WHISTLE)
                audio.play(MatchAudioManager.Cue.POST_MATCH)
            }
            LabEventType.SUBSTITUTION -> audio.play(MatchAudioManager.Cue.SUBSTITUTION)
            LabEventType.HALF_TIME, LabEventType.EXTRA_TIME_HALF_TIME -> audio.playHalfTime()
            LabEventType.FULL_TIME -> audio.playFullTime()
            else -> Unit
        }
    }

    private fun decayCrowd(simTimeMs: Long) {
        val elapsed = simTimeMs - lastTransitionSimTimeMs
        // Highlights skip (or backward seek on restore): reset directly to CALM.
        if (elapsed > DECAY_MS * 2L || elapsed < 0L) {
            if (crowdState != CrowdState.CALM) {
                crowdState = CrowdState.CALM
                audio.setAmbienceVolume(crowdState.ambienceVolume)
                LabLogStore.log("AUDIO", "CrowdState jump-reset -> CALM (simTimeMs=$simTimeMs)")
            }
            lastTransitionSimTimeMs = simTimeMs
            return
        }
        if (elapsed < DECAY_MS) return
        val next = when (crowdState) {
            CrowdState.RESOLUTION, CrowdState.LATE_TENSION -> CrowdState.CALM
            CrowdState.BIG_CHANCE -> CrowdState.DANGEROUS_ATTACK
            CrowdState.DANGEROUS_ATTACK -> CrowdState.ATTACK
            CrowdState.ATTACK -> CrowdState.BUILDING
            CrowdState.BUILDING -> CrowdState.CALM
            CrowdState.CALM -> CrowdState.CALM
        }
        if (next != crowdState) {
            crowdState = next
            lastTransitionSimTimeMs = simTimeMs
            audio.setAmbienceVolume(crowdState.ambienceVolume)
        }
    }

    private fun allowCooldown(kind: LabEventType, simTimeMs: Long): Boolean {
        val cooldown = when (kind) {
            LabEventType.GOAL, LabEventType.OWN_GOAL -> 150L
            LabEventType.PASS_COMPLETED, LabEventType.THROUGH_BALL -> 220L
            LabEventType.FOUL, LabEventType.OFFSIDE -> 700L
            LabEventType.SHOT, LabEventType.SHOT_ON_TARGET, LabEventType.SAVE -> 60L
            else -> 80L
        }
        val prev = lastCueSimTimeMs[kind]
        if (prev != null && simTimeMs >= prev && simTimeMs - prev < cooldown) return false
        lastCueSimTimeMs[kind] = simTimeMs
        return true
    }
}
