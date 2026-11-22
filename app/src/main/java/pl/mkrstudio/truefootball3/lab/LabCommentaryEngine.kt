package pl.mkrstudio.truefootball3.lab

import android.content.Context
import org.json.JSONObject
import kotlin.math.abs

class LabCommentaryEngine(
    private val context: Context,
    private val homeTeam: String,
    private val awayTeam: String,
    private val seed: Long,
    private val tracker: LabNarrativeTracker = LabNarrativeTracker()
) {
    private val groupCache = HashMap<String, List<Pair<String, String>>>()

    private val zoneLeft: String by lazy { loadSingleString("cv2_zone_left", "hành lang cánh trái") }
    private val zoneRight: String by lazy { loadSingleString("cv2_zone_right", "hành lang cánh phải") }
    private val zoneCenter: String by lazy { loadSingleString("cv2_zone_center", "khu vực trung lộ") }

    private fun loadSingleString(name: String, fallback: String): String {
        val id = context.resources.getIdentifier(name, "string", context.packageName)
        return if (id != 0) context.getString(id) else fallback
    }

    private fun templatesFor(prefix: String): List<Pair<String, String>> {
        groupCache[prefix]?.let { return it }
        val found = ArrayList<Pair<String, String>>()
        val res = context.resources
        val pkg = context.packageName
        for (i in 1..30) {
            val key = "${prefix}_$i"
            val id = res.getIdentifier(key, "string", pkg)
            if (id == 0) {
                if (i > 1) break
            } else {
                found.add(key to res.getString(id))
            }
        }
        groupCache[prefix] = found
        return found
    }

    private fun templatesForAny(prefixes: List<String>): List<Pair<String, String>> {
        val combined = ArrayList<Pair<String, String>>()
        for (p in prefixes) {
            combined.addAll(templatesFor(p))
        }
        return combined
    }

    fun renderRustEvent(
        ev: JSONObject,
        kind: LabEventType,
        side: LabTeamSide?,
        actorName: String,
        targetName: String,
        minute: Int,
        homeGoals: Int,
        awayGoals: Int
    ): String? {
        val gameSecond = ev.optInt("gameSecond", minute * 60)
        val tacticalPhase = ev.optString("tacticalPhase", "PROGRESSION")
        val ballX = ev.optInt("ballX", 50)
        val prevBallX = tracker.lastBallX
        val wasPressed = tracker.wasRecentlyPressed(gameSecond)
        val wasShotOrRebound = tracker.wasRecentShotOrRebound(gameSecond)
        val meta = ev.optJSONObject("metadata")

        if (kind == LabEventType.GOAL) {
            val isOwnGoal = meta?.optString("ownGoal") == "true"
            val situation = tracker.classifyGoal(
                scoringSide = side,
                scorerId = ev.optString("actor", "").takeIf { it != "null" },
                homeGoalsAfter = homeGoals,
                awayGoalsAfter = awayGoals,
                minute = minute,
                isOwnGoal = isOwnGoal
            )
            tracker.recordContext(kind, side, ballX, gameSecond)
            if (isOwnGoal) return null
            val prefixes = when (situation) {
                LabNarrativeTracker.GoalSituation.HAT_TRICK -> listOf("cv3_goal_hat_trick")
                LabNarrativeTracker.GoalSituation.OPENING -> listOf("cv3_goal_opening", "cv3_goal_player")
                LabNarrativeTracker.GoalSituation.EQUALIZER -> listOf("cv3_goal_equalizer")
                LabNarrativeTracker.GoalSituation.COMEBACK_LEAD -> listOf("cv3_goal_comeback_lead")
                LabNarrativeTracker.GoalSituation.LATE_GO_AHEAD -> listOf("cv3_goal_late_go_ahead")
                LabNarrativeTracker.GoalSituation.BREAKS_TIE -> listOf("cv3_goal_breaks_tie", "cv3_goal_player")
                LabNarrativeTracker.GoalSituation.EXTENDS_LEAD -> listOf("cv3_goal_extends_lead", "cv3_goal_player")
                LabNarrativeTracker.GoalSituation.NARROWS_DEFICIT -> listOf("cv3_goal_narrows_deficit")
                LabNarrativeTracker.GoalSituation.GENERIC -> {
                    if (wasShotOrRebound) listOf("cv2_scramble_goal", "cv3_goal_generic", "cv3_goal_player")
                    else listOf("cv3_goal_generic", "cv3_goal_player")
                }
            }
            val base = pickAndFormat(prefixes, ev, kind, side, actorName, targetName, minute, homeGoals, awayGoals, meta)
                ?: return null
            val scoreClause = if (!base.contains("$homeGoals-$awayGoals") && !base.contains("$homeGoals – $awayGoals")) {
                pickAndFormat(listOf("cv3_score", "cv2_score"), ev, kind, side, actorName, targetName, minute, homeGoals, awayGoals, meta, salt = 31L)
            } else null
            return if (scoreClause != null) "$base $scoreClause" else base
        } else {
            tracker.syncScore(homeGoals, awayGoals)
        }

        val speak = tracker.shouldSpeak(kind, gameSecond, tacticalPhase)
        val sustained = tracker.recordAttackMomentum(side, kind)
        tracker.recordContext(kind, side, ballX, gameSecond)
        if (!speak) return null

        val prefixes = selectPrefixes(
            kind = kind,
            side = side,
            meta = meta,
            tacticalPhase = tacticalPhase,
            minute = minute,
            ballX = ballX,
            prevBallX = prevBallX,
            homeGoals = homeGoals,
            awayGoals = awayGoals,
            sustainedPressure = sustained,
            wasPressed = wasPressed,
            wasShotOrRebound = wasShotOrRebound
        )
        if (prefixes.isEmpty()) return null

        val mainLine = pickAndFormat(prefixes, ev, kind, side, actorName, targetName, minute, homeGoals, awayGoals, meta)
            ?: return null

        val extraClause = selectContextualFollowUp(
            kind = kind,
            side = side,
            minute = minute,
            gameSecond = gameSecond,
            homeGoals = homeGoals,
            awayGoals = awayGoals,
            ev = ev,
            actorName = actorName,
            targetName = targetName,
            meta = meta
        )
        return if (extraClause != null) "$mainLine $extraClause" else mainLine
    }

    private fun selectPrefixes(
        kind: LabEventType,
        side: LabTeamSide?,
        meta: JSONObject?,
        tacticalPhase: String,
        minute: Int,
        ballX: Int,
        prevBallX: Int,
        homeGoals: Int,
        awayGoals: Int,
        sustainedPressure: Boolean,
        wasPressed: Boolean,
        wasShotOrRebound: Boolean
    ): List<String> {
        val ownLeading = isSideLeading(side, homeGoals, awayGoals)
        val onFlank = ballX <= 28 || ballX >= 72
        val isSwitchPlay = abs(ballX - prevBallX) >= 32 || meta?.optString("passType") == "SWITCH"

        return when (kind) {
            LabEventType.MATCH_STARTED, LabEventType.KICKOFF -> listOf("cv3_match_phase")
            LabEventType.TACTICAL_PHASE_CHANGED -> {
                when {
                    tracker.isTenMen(side) -> listOf("cv3_ten_men", "cv3_match_phase")
                    tacticalPhase == "HIGH_PRESS" || tacticalPhase == "FINAL_THIRD" -> listOf("cv3_high_line", "cv3_match_phase")
                    else -> listOf("cv3_match_phase")
                }
            }
            LabEventType.POSSESSION_STARTED -> listOf("cv3_possession", "cv2_possession")
            LabEventType.PRESS -> {
                if (minute >= 72) listOf("cv3_press", "cv3_fatigue") else listOf("cv3_press")
            }
            LabEventType.TURNOVER -> listOf("cv3_transition", "cv2_counter")
            LabEventType.TRANSITION_STARTED -> listOf("cv3_counter_attack", "cv2_counter")
            LabEventType.PASS_COMPLETED -> {
                val gain = meta?.optString("progressGain")?.toIntOrNull() ?: 0
                when {
                    wasPressed -> listOf("cv3_escape_press", "cv3_progressive_pass")
                    isSwitchPlay -> listOf("cv3_switch_play", "cv2_direction")
                    sustainedPressure && onFlank -> listOf("cv3_overload", "cv3_sustained_pressure")
                    sustainedPressure && tacticalPhase == "FINAL_THIRD" -> listOf("cv3_sustained_pressure")
                    tacticalPhase == "BOX_ATTACK" -> listOf("cv3_box_attack", "cv2_box_attack")
                    gain >= 12 -> listOf("cv3_progressive_pass", "cv2_direction")
                    else -> listOf("cv3_build_up", "cv2_build_up")
                }
            }
            LabEventType.PASS_FAILED -> {
                if (minute >= 74) listOf("cv3_ball_loss", "cv2_ball_loss", "cv3_fatigue")
                else listOf("cv3_ball_loss", "cv2_ball_loss")
            }
            LabEventType.THROUGH_BALL -> listOf("cv3_through_ball", "cv3_high_line")
            LabEventType.FIRST_TOUCH -> {
                if (wasPressed) listOf("cv3_escape_press", "cv3_first_touch") else listOf("cv3_first_touch")
            }
            LabEventType.CARRY, LabEventType.DRIBBLE_STARTED -> {
                if (minute >= 76) listOf("cv3_dribble", "cv2_dribble", "cv3_fatigue")
                else listOf("cv3_dribble", "cv2_dribble")
            }
            LabEventType.DRIBBLE_WON -> {
                if (wasPressed) listOf("cv3_escape_press", "cv3_dribble")
                else listOf("cv3_dribble", "cv2_dribble")
            }
            LabEventType.DRIBBLE_LOST -> listOf("cv3_ball_loss", "cv2_ball_loss")
            LabEventType.CROSS -> listOf("cv3_cross", "cv2_cross")
            LabEventType.TACKLE_ATTEMPTED, LabEventType.TACKLE_WON,
            LabEventType.TACKLE_LOST -> {
                if (tracker.isTenMen(side)) listOf("cv3_defensive_recovery", "cv3_ten_men")
                else listOf("cv3_defensive_recovery")
            }
            LabEventType.INTERCEPTION -> listOf("cv3_interception", "cv2_interception")
            LabEventType.BLOCK -> listOf("cv3_block")
            LabEventType.CLEARANCE -> listOf("cv3_clearance")
            LabEventType.GROUND_DUEL, LabEventType.AERIAL_DUEL -> listOf("cv3_duel")
            LabEventType.LOOSE_BALL, LabEventType.SECOND_BALL_WON -> {
                if (tacticalPhase == "BOX_ATTACK") listOf("cv2_chaos", "cv3_loose_ball")
                else listOf("cv3_loose_ball")
            }
            LabEventType.REBOUND -> {
                if (wasShotOrRebound) listOf("cv2_second_shot", "cv3_rebound", "cv2_chaos")
                else listOf("cv3_rebound", "cv2_chaos")
            }
            LabEventType.SHOT -> {
                val dist = meta?.optString("distance")?.toDoubleOrNull() ?: 16.0
                when {
                    meta?.optString("oneVsOne") == "true" -> listOf("cv2_one_on_one")
                    wasShotOrRebound -> listOf("cv2_second_shot", "cv3_shot", "cv2_shot")
                    dist >= 22.0 -> listOf("cv2_distance_shot", "cv3_shot")
                    meta?.optString("shotType") == "CURLED" || meta?.optString("finesse") == "true" ->
                        listOf("cv2_technical", "cv3_shot")
                    else -> listOf("cv3_shot", "cv2_shot", "cv2_technical")
                }
            }
            LabEventType.SHOT_ON_TARGET -> listOf("cv3_shot_on_target", "cv2_shot")
            LabEventType.HEADER -> listOf("cv3_header", "cv2_header")
            LabEventType.SHOT_OFF_TARGET -> {
                val isWoodwork = meta?.optString("woodwork") == "true" ||
                    meta?.optString("outcome")?.equals("WOODWORK", ignoreCase = true) == true
                if (isWoodwork) listOf("cv2_woodwork") else listOf("cv3_missed", "cv2_missed")
            }
            LabEventType.OWN_GOAL -> listOf("cv3_own_goal", "cv2_own_goal")
            LabEventType.SAVE -> listOf("cv3_keeper_save", "cv2_keeper_save", "cv2_keeper_reflect")
            LabEventType.GOALKEEPER_PUNCH -> listOf("cv3_goalkeeper_punch", "cv2_keeper_reflect")
            LabEventType.GOALKEEPER_DISTRIBUTION -> listOf("cv3_goalkeeper_distribution")
            LabEventType.GOALKEEPER_CLAIM, LabEventType.GOALKEEPER_RUSH,
            LabEventType.GOALKEEPER_ASSIGNMENT_REQUIRED, LabEventType.GOALKEEPER_REASSIGNED ->
                listOf("cv3_goalkeeper_action")
            LabEventType.FOUL -> listOf("cv3_foul")
            LabEventType.ADVANTAGE_PLAYED -> listOf("cv3_advantage")
            LabEventType.YELLOW_CARD -> listOf("cv3_yellow_card", "cv2_yellow_card")
            LabEventType.RED_CARD -> {
                if (meta?.optString("secondYellow") == "true") listOf("cv3_second_yellow", "cv2_second_yellow")
                else listOf("cv3_red_card", "cv2_red_card")
            }
            LabEventType.OFFSIDE -> listOf("cv3_offside", "cv2_offside", "cv3_high_line")
            LabEventType.INJURY -> listOf("cv3_injury", "cv2_injury")
            LabEventType.THROW_IN -> {
                if (minute >= 78 && ownLeading) listOf("cv3_time_wasting", "cv3_throw_in")
                else listOf("cv3_throw_in")
            }
            LabEventType.GOAL_KICK -> {
                if (minute >= 78 && ownLeading) listOf("cv3_time_wasting", "cv3_goal_kick")
                else listOf("cv3_goal_kick")
            }
            LabEventType.CORNER -> listOf("cv3_corner", "cv2_corner")
            LabEventType.FREE_KICK -> {
                if (minute >= 80 && ownLeading) listOf("cv3_time_wasting", "cv3_free_kick")
                else listOf("cv3_free_kick", "cv2_free_kick", "cv2_free_kick_execution")
            }
            LabEventType.PENALTY_AWARDED -> listOf("cv3_penalty_awarded")
            LabEventType.PENALTY_KICK -> listOf("cv3_penalty_kick")
            LabEventType.SUBSTITUTION -> listOf("cv3_substitution")
            LabEventType.COMMAND_ACCEPTED -> listOf("cv3_tactical_change")
            LabEventType.ADDED_TIME_ANNOUNCED -> listOf("cv3_added_time")
            LabEventType.HALF_TIME -> listOf("cv3_half_time", "cv2_half_time")
            LabEventType.EXTRA_TIME_STARTED -> listOf("cv3_extra_start", "cv2_extra_start")
            LabEventType.EXTRA_TIME_HALF_TIME -> listOf("cv3_extra_half", "cv2_extra_half")
            LabEventType.PENALTY_SHOOTOUT_STARTED -> listOf("cv3_shootout_start", "cv2_penalties_start")
            LabEventType.PENALTY_SHOOTOUT_KICK -> listOf("cv3_shootout_kick")
            LabEventType.PENALTY_SHOOTOUT_ENDED -> listOf("cv3_shootout_result", "cv2_penalties_result")
            LabEventType.FULL_TIME -> when {
                tracker.shootoutOccurred -> listOf("cv3_full_shootout", "cv2_penalties_result")
                homeGoals == awayGoals -> listOf("cv3_full_draw", "cv2_full_draw")
                else -> listOf("cv3_full_win", "cv2_full_win")
            }
            else -> emptyList()
        }
    }

    private fun selectContextualFollowUp(
        kind: LabEventType,
        side: LabTeamSide?,
        minute: Int,
        gameSecond: Int,
        homeGoals: Int,
        awayGoals: Int,
        ev: JSONObject,
        actorName: String,
        targetName: String,
        meta: JSONObject?
    ): String? {

        val isRoutineBeat = kind == LabEventType.PASS_COMPLETED ||
            kind == LabEventType.POSSESSION_STARTED ||
            kind == LabEventType.CARRY
        if (!isRoutineBeat) return null

        if (tracker.shouldInjectStateFlavor(gameSecond)) {
            val ownLeading = isSideLeading(side, homeGoals, awayGoals)
            val ownTrailing = isSideTrailing(side, homeGoals, awayGoals)
            val statePrefixes = when {
                tracker.isTenMen(side) -> listOf("cv3_ten_men")
                minute <= 12 && homeGoals == 0 && awayGoals == 0 -> listOf("cv2_early_state")
                minute >= 75 && ownLeading -> listOf("cv2_late_leading")
                minute >= 75 && ownTrailing -> listOf("cv2_late_trailing")
                minute >= 75 && homeGoals == awayGoals -> listOf("cv2_late_level")
                ownLeading -> listOf("cv2_control")
                ownTrailing -> listOf("cv2_chasing")
                else -> emptyList()
            }
            if (statePrefixes.isNotEmpty()) {
                return pickAndFormat(statePrefixes, ev, kind, side, actorName, targetName, minute, homeGoals, awayGoals, meta, salt = 73L)
            }
        }

        if (minute >= 20 && tracker.shouldAppendScore(gameSecond)) {
            return pickAndFormat(listOf("cv3_score", "cv2_score"), ev, kind, side, actorName, targetName, minute, homeGoals, awayGoals, meta, salt = 97L)
        }
        return null
    }

    private fun isSideLeading(side: LabTeamSide?, homeGoals: Int, awayGoals: Int): Boolean = when (side) {
        LabTeamSide.HOME -> homeGoals > awayGoals
        LabTeamSide.AWAY -> awayGoals > homeGoals
        null -> false
    }

    private fun isSideTrailing(side: LabTeamSide?, homeGoals: Int, awayGoals: Int): Boolean = when (side) {
        LabTeamSide.HOME -> homeGoals < awayGoals
        LabTeamSide.AWAY -> awayGoals < homeGoals
        null -> false
    }

    private fun pickAndFormat(
        prefixes: List<String>,
        ev: JSONObject,
        kind: LabEventType,
        side: LabTeamSide?,
        actorName: String,
        targetName: String,
        minute: Int,
        homeGoals: Int,
        awayGoals: Int,
        meta: JSONObject?,
        salt: Long = 0L
    ): String? {
        val candidates = templatesForAny(prefixes)
        if (candidates.isEmpty()) return null
        val seq = ev.optLong("sequence", 0L)
        val fresh = candidates.filter { !tracker.isTemplateOnCooldown(it.first) }
        val pool = if (fresh.isNotEmpty()) fresh else candidates
        val hash = (((seed xor salt) xor (seq * 2654435761L)) ushr 1).toInt()
        val chosen = pool[Math.floorMod(hash, pool.size)]
        tracker.markTemplateUsed(chosen.first)

        val teamName = when (side) {
            LabTeamSide.HOME -> homeTeam
            LabTeamSide.AWAY -> awayTeam
            null -> homeTeam
        }
        val opponentName = when (side) {
            LabTeamSide.HOME -> awayTeam
            LabTeamSide.AWAY -> homeTeam
            null -> awayTeam
        }
        val ballX = ev.optInt("ballX", 50)
        val zone = when {
            ballX <= 28 -> zoneLeft
            ballX >= 72 -> zoneRight
            else -> zoneCenter
        }
        val addedSec = meta?.optString("seconds")?.toIntOrNull() ?: 60
        val addedMin = ((addedSec + 59) / 60).coerceAtLeast(1)
        val shootout = meta?.optString("shootout", "0-0") ?: "0-0"
        val shootoutParts = shootout.split("-")
        val penHome = shootoutParts.getOrNull(0) ?: "0"
        val penAway = shootoutParts.getOrNull(1) ?: "0"

        val winner = when {
            homeGoals > awayGoals -> homeTeam
            awayGoals > homeGoals -> awayTeam
            (penHome.toIntOrNull() ?: 0) >= (penAway.toIntOrNull() ?: 0) -> homeTeam
            else -> awayTeam
        }
        val loser = if (winner == homeTeam) awayTeam else homeTeam

        val safeActor = actorName.ifBlank { teamName }
        val safeTarget = targetName.ifBlank { opponentName }
        val isDefensiveEvent = kind == LabEventType.BLOCK ||
            kind == LabEventType.CLEARANCE ||
            kind == LabEventType.INTERCEPTION ||
            kind == LabEventType.TACKLE_WON ||
            kind == LabEventType.TACKLE_ATTEMPTED ||
            kind == LabEventType.SAVE ||
            kind == LabEventType.GOALKEEPER_CLAIM ||
            kind == LabEventType.GOALKEEPER_PUNCH

        val attackingTeam = if (isDefensiveEvent) opponentName else teamName
        val defendingTeam = if (isDefensiveEvent) teamName else opponentName

        val values = mapOf(
            "PLAYER" to safeActor,
            "PASSER" to safeActor,
            "SHOOTER" to safeActor,
            "DEFENDER" to safeActor,
            "GOALKEEPER" to safeActor,
            "RECEIVER" to safeTarget,
            "PRESSURED_PLAYER" to safeTarget,
            "PLAYER_OUT" to safeActor,
            "PLAYER_IN" to safeTarget,
            "TEAM" to teamName,
            "OPPONENT" to opponentName,
            "ATTACKING_TEAM" to attackingTeam,
            "DEFENDING_TEAM" to defendingTeam,
            "HOME_TEAM" to homeTeam,
            "AWAY_TEAM" to awayTeam,
            "WINNER" to winner,
            "LOSER" to loser,
            "MINUTE" to minute.toString(),
            "ADDED_TIME" to addedMin.toString(),
            "HOME_SCORE_AFTER" to homeGoals.toString(),
            "AWAY_SCORE_AFTER" to awayGoals.toString(),
            "SCORE_AFTER" to "$homeGoals-$awayGoals",
            "TO_ZONE" to zone,
            "FLANK" to zone,
            "SHOOTOUT_RESULT" to shootout,
            "PENALTY_HOME_SCORE" to penHome,
            "PENALTY_AWAY_SCORE" to penAway
        )

        var rendered = chosen.second
        for ((k, v) in values) {
            rendered = rendered.replace("{$k}", v)
        }
        return rendered.trim().takeIf { it.isNotEmpty() && !it.contains('{') }
    }
}
