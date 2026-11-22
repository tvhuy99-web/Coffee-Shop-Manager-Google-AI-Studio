package pl.mkrstudio.truefootball3.lab

import pl.mkrstudio.truefootball3.audio.MatchAudioManager

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

    private var crowdState: CrowdState = CrowdState.CALM
    private var lastTransitionMs: Long = 0L
    private var lastCrowdRiseMs: Long = 0L
    private val lastCueMs = HashMap<LabEventType, Long>()

    init {
        audio.setStadiumCapacity(stadiumCapacity)
    }

    companion object {
        private const val HYSTERESIS_MS = 2500L
        private const val DECAY_MS = 6500L
        private const val CROWD_RISE_COOLDOWN_MS = 3200L
    }

    fun currentCrowdState(): CrowdState = crowdState

    fun onEvent(
        kind: LabEventType,
        side: LabTeamSide?,
        minute: Int,
        goalDiffAbs: Int,
        nowMs: Long,
        isWoodwork: Boolean = false,
        isPenalty: Boolean = false,
        isShootoutScored: Boolean? = null
    ) {
        decayCrowd(nowMs)

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
        if (targetState != crowdState && (escalate || nowMs - lastTransitionMs >= HYSTERESIS_MS)) {
            val prevRank = crowdState.rank
            crowdState = targetState
            lastTransitionMs = nowMs
            audio.setAmbienceVolume(crowdState.ambienceVolume)
            LabLogStore.log("AUDIO", "CrowdState -> $crowdState (ambience=${crowdState.ambienceVolume})")
            if (targetState.rank >= CrowdState.DANGEROUS_ATTACK.rank &&
                targetState != CrowdState.RESOLUTION &&
                prevRank < targetState.rank &&
                nowMs - lastCrowdRiseMs >= CROWD_RISE_COOLDOWN_MS
            ) {
                LabLogStore.log("AUDIO", "Play Cue.CROWD_RISE (min=$minute)")
                audio.play(MatchAudioManager.Cue.CROWD_RISE)
                lastCrowdRiseMs = nowMs
            }
        }

        if (!allowCooldown(kind, nowMs)) return

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
                    isWoodwork -> { LabLogStore.log("AUDIO", "Play Cue.POST_CROSSBAR (min=$minute)"); audio.play(MatchAudioManager.Cue.POST_CROSSBAR) }
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

    private fun decayCrowd(nowMs: Long) {
        if (nowMs - lastTransitionMs < DECAY_MS) return
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
            lastTransitionMs = nowMs
            audio.setAmbienceVolume(crowdState.ambienceVolume)
        }
    }

    private fun allowCooldown(kind: LabEventType, nowMs: Long): Boolean {
        val cooldown = when (kind) {
            LabEventType.GOAL, LabEventType.OWN_GOAL -> 150L
            LabEventType.PASS_COMPLETED, LabEventType.THROUGH_BALL -> 220L
            LabEventType.FOUL, LabEventType.OFFSIDE -> 700L
            LabEventType.SHOT, LabEventType.SHOT_ON_TARGET, LabEventType.SAVE -> 60L
            else -> 80L
        }
        val prev = lastCueMs[kind]
        if (prev != null && nowMs - prev < cooldown) return false
        lastCueMs[kind] = nowMs
        return true
    }
}
