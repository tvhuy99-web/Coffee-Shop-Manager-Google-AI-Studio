package pl.mkrstudio.truefootball3.lab

import java.util.ArrayDeque

class LabNarrativeTracker {
    enum class GoalSituation {
        HAT_TRICK,
        OPENING,
        EQUALIZER,
        COMEBACK_LEAD,
        LATE_GO_AHEAD,
        BREAKS_TIE,
        EXTENDS_LEAD,
        NARROWS_DEFICIT,
        GENERIC
    }

    private var prevHomeGoals = 0
    private var prevAwayGoals = 0
    private var homeEverTrailed = false
    private var awayEverTrailed = false
    private var consecutiveAttackTeam: LabTeamSide? = null
    private var consecutiveAttackEvents = 0
    private val playerGoals = HashMap<String, Int>()
    private val recentTemplates = ArrayDeque<String>()
    private val lastSpokenSecondByType = HashMap<LabEventType, Int>()

    var homeRedCards: Int = 0
        private set
    var awayRedCards: Int = 0
        private set
    var lastBallX: Int = 50
        private set
    var lastEventKind: LabEventType? = null
        private set
    var lastEventSecond: Int = -999
        private set
    var shootoutOccurred: Boolean = false
        private set
    var lastScoreAnnouncedSecond: Int = -999
        private set
    var lastStateFlavorSecond: Int = -999
        private set

    companion object {
        private const val TEMPLATE_COOLDOWN_WINDOW = 14
    }

    fun recordContext(kind: LabEventType, side: LabTeamSide?, ballX: Int, gameSecond: Int) {
        if (kind == LabEventType.RED_CARD) {
            if (side == LabTeamSide.HOME) homeRedCards++ else if (side == LabTeamSide.AWAY) awayRedCards++
        }
        if (kind == LabEventType.PENALTY_SHOOTOUT_STARTED ||
            kind == LabEventType.PENALTY_SHOOTOUT_KICK ||
            kind == LabEventType.PENALTY_SHOOTOUT_ENDED
        ) {
            shootoutOccurred = true
        }
        lastEventKind = kind
        lastEventSecond = gameSecond
        lastBallX = ballX
    }

    fun isTenMen(side: LabTeamSide?): Boolean = when (side) {
        LabTeamSide.HOME -> homeRedCards > 0
        LabTeamSide.AWAY -> awayRedCards > 0
        null -> false
    }

    fun wasRecentlyPressed(gameSecond: Int): Boolean =
        (lastEventKind == LabEventType.PRESS || lastEventKind == LabEventType.PRESSURE_APPLIED) &&
            (gameSecond - lastEventSecond <= 15)

    fun wasRecentShotOrRebound(gameSecond: Int): Boolean =
        (lastEventKind == LabEventType.SHOT ||
            lastEventKind == LabEventType.SHOT_ON_TARGET ||
            lastEventKind == LabEventType.REBOUND ||
            lastEventKind == LabEventType.SAVE ||
            lastEventKind == LabEventType.BLOCK) &&
            (gameSecond - lastEventSecond <= 18)

    fun shouldAppendScore(gameSecond: Int): Boolean {
        if (gameSecond - lastScoreAnnouncedSecond < 300) return false
        lastScoreAnnouncedSecond = gameSecond
        return true
    }

    fun shouldInjectStateFlavor(gameSecond: Int): Boolean {
        if (gameSecond - lastStateFlavorSecond < 240) return false
        lastStateFlavorSecond = gameSecond
        return true
    }

    fun shouldSpeak(kind: LabEventType, gameSecond: Int, tacticalPhase: String): Boolean {
        when (kind) {
            LabEventType.MATCH_STARTED,
            LabEventType.GOAL, LabEventType.OWN_GOAL,
            LabEventType.SHOT, LabEventType.SHOT_ON_TARGET, LabEventType.SHOT_OFF_TARGET,
            LabEventType.HEADER, LabEventType.SAVE, LabEventType.BLOCK,
            LabEventType.PENALTY_AWARDED, LabEventType.PENALTY_KICK,
            LabEventType.YELLOW_CARD, LabEventType.RED_CARD,
            LabEventType.OFFSIDE, LabEventType.INJURY,
            LabEventType.SUBSTITUTION, LabEventType.COMMAND_ACCEPTED,
            LabEventType.ADDED_TIME_ANNOUNCED,
            LabEventType.HALF_TIME, LabEventType.EXTRA_TIME_STARTED,
            LabEventType.EXTRA_TIME_HALF_TIME,
            LabEventType.PENALTY_SHOOTOUT_STARTED,
            LabEventType.PENALTY_SHOOTOUT_KICK,
            LabEventType.PENALTY_SHOOTOUT_ENDED,
            LabEventType.FULL_TIME -> {
                lastSpokenSecondByType[kind] = gameSecond
                return true
            }
            LabEventType.PRESSURE_APPLIED,
            LabEventType.PASS_STARTED,
            LabEventType.TRANSITION_ENDED,
            LabEventType.RESTART_SCHEDULED,
            LabEventType.COMMAND_REJECTED -> return false
            else -> Unit
        }

        val minGapSeconds = when (kind) {
            LabEventType.CROSS, LabEventType.THROUGH_BALL,
            LabEventType.CORNER, LabEventType.FREE_KICK -> 25
            LabEventType.DRIBBLE_WON, LabEventType.INTERCEPTION,
            LabEventType.TACKLE_WON, LabEventType.CLEARANCE,
            LabEventType.REBOUND, LabEventType.GOALKEEPER_CLAIM,
            LabEventType.GOALKEEPER_PUNCH, LabEventType.GOALKEEPER_RUSH -> 45
            LabEventType.PASS_COMPLETED, LabEventType.CARRY,
            LabEventType.FIRST_TOUCH, LabEventType.PRESS,
            LabEventType.TURNOVER, LabEventType.TRANSITION_STARTED -> {
                if (tacticalPhase == "FINAL_THIRD" || tacticalPhase == "BOX_ATTACK") 75 else 135
            }
            else -> 160
        }
        val last = lastSpokenSecondByType[kind]
        if (last != null && gameSecond - last < minGapSeconds) return false
        lastSpokenSecondByType[kind] = gameSecond
        return true
    }

    fun recordAttackMomentum(side: LabTeamSide?, kind: LabEventType): Boolean {
        if (side == null) return false
        val isAttackingAction = kind == LabEventType.PASS_COMPLETED ||
            kind == LabEventType.CROSS ||
            kind == LabEventType.THROUGH_BALL ||
            kind == LabEventType.SHOT ||
            kind == LabEventType.CORNER
        if (!isAttackingAction) return false
        if (consecutiveAttackTeam == side) {
            consecutiveAttackEvents++
        } else {
            consecutiveAttackTeam = side
            consecutiveAttackEvents = 1
        }
        return consecutiveAttackEvents >= 4
    }

    fun classifyGoal(
        scoringSide: LabTeamSide?,
        scorerId: String?,
        homeGoalsAfter: Int,
        awayGoalsAfter: Int,
        minute: Int,
        isOwnGoal: Boolean
    ): GoalSituation {
        val beforeHome = prevHomeGoals
        val beforeAway = prevAwayGoals
        prevHomeGoals = homeGoalsAfter
        prevAwayGoals = awayGoalsAfter
        if (homeGoalsAfter < awayGoalsAfter) homeEverTrailed = true
        if (awayGoalsAfter < homeGoalsAfter) awayEverTrailed = true

        if (!isOwnGoal && !scorerId.isNullOrEmpty()) {
            val count = (playerGoals[scorerId] ?: 0) + 1
            playerGoals[scorerId] = count
            if (count >= 3) return GoalSituation.HAT_TRICK
        }
        if (beforeHome == 0 && beforeAway == 0) return GoalSituation.OPENING
        if (homeGoalsAfter == awayGoalsAfter) return GoalSituation.EQUALIZER

        val side = scoringSide ?: return GoalSituation.GENERIC
        val ownBefore = if (side == LabTeamSide.HOME) beforeHome else beforeAway
        val oppBefore = if (side == LabTeamSide.HOME) beforeAway else beforeHome
        val ownAfter = if (side == LabTeamSide.HOME) homeGoalsAfter else awayGoalsAfter
        val oppAfter = if (side == LabTeamSide.HOME) awayGoalsAfter else homeGoalsAfter
        val everTrailed = if (side == LabTeamSide.HOME) homeEverTrailed else awayEverTrailed

        if (ownBefore == oppBefore && ownAfter > oppAfter) {
            if (everTrailed) return GoalSituation.COMEBACK_LEAD
            if (minute >= 78) return GoalSituation.LATE_GO_AHEAD
            return GoalSituation.BREAKS_TIE
        }
        if (ownBefore > oppBefore) return GoalSituation.EXTENDS_LEAD
        if (ownAfter < oppAfter) return GoalSituation.NARROWS_DEFICIT
        return GoalSituation.GENERIC
    }

    fun syncScore(homeGoals: Int, awayGoals: Int) {
        if (homeGoals > prevHomeGoals || awayGoals > prevAwayGoals) {
            prevHomeGoals = homeGoals
            prevAwayGoals = awayGoals
            if (homeGoals < awayGoals) homeEverTrailed = true
            if (awayGoals < homeGoals) awayEverTrailed = true
        }
    }

    fun isTemplateOnCooldown(key: String): Boolean = recentTemplates.contains(key)

    fun markTemplateUsed(key: String) {
        recentTemplates.addLast(key)
        while (recentTemplates.size > TEMPLATE_COOLDOWN_WINDOW) {
            recentTemplates.removeFirst()
        }
    }
}
