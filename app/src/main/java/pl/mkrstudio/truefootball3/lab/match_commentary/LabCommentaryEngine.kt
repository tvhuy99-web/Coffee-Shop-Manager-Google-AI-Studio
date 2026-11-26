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
        // The JSON bridge uses empty strings for absent names. Treat literal "null" as absent.
        val safeActorName = actorName.trim().takeUnless { it.equals("null", true) }.orEmpty()
        val safeTargetName = targetName.trim().takeUnless { it.equals("null", true) }.orEmpty()
        val gameSecond = ev.optInt("gameSecond", minute * 60)
        val tacticalPhase = ev.optString("tacticalPhase", "PROGRESSION")
        val ballX = ev.optInt("ballX", 50)
        val lastKindBeforeEvent = tracker.lastEventKind
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
            val base = pickAndFormat(prefixes, ev, kind, side, safeActorName, safeTargetName, minute, homeGoals, awayGoals, meta)
                ?: if (side != null) factualText("lab_commentary_goal_generic", mapOf(
                    "TEAM" to teamName(side), "SCORE_AFTER" to "$homeGoals-$awayGoals"
                )) else factualText("lab_commentary_goal_generic_unknown_side", mapOf(
                    "SCORE_AFTER" to "$homeGoals-$awayGoals"
                ))
            if (base == null || base.isBlank()) return null
            val scoreClause = if (!base.contains("$homeGoals-$awayGoals") && !base.contains("$homeGoals – $awayGoals")) {
                pickAndFormat(listOf("cv3_score", "cv2_score"), ev, kind, side, safeActorName, safeTargetName, minute, homeGoals, awayGoals, meta, salt = 31L)
            } else null
            return if (scoreClause != null) "$base $scoreClause" else base
        } else {
            tracker.syncScore(homeGoals, awayGoals)
        }

        val speak = tracker.shouldSpeak(kind, gameSecond, tacticalPhase)
        tracker.recordContext(kind, side, ballX, gameSecond)
        if (!speak) return null

        val prefixes = selectPrefixes(
            kind = kind,
            side = side,
            meta = meta,
            minute = minute,
            homeGoals = homeGoals,
            awayGoals = awayGoals,
            wasShotOrRebound = wasShotOrRebound
        )
        // A goalkeeper claim followed by POSSESSION_STARTED is still goalkeeper
        // possession, even if the Rust event exposes a generic field coordinate.
        if (kind == LabEventType.POSSESSION_STARTED &&
            lastKindBeforeEvent == LabEventType.GOALKEEPER_CLAIM && safeActorName.isNotBlank()
        ) {
            return factualText("lab_commentary_keeper_possession", mapOf(
                "GOALKEEPER" to safeActorName, "TEAM" to teamName(side)
            ))
        }
        if (prefixes.isEmpty()) return factualFallback(kind, side, safeActorName)

        val mainLine = pickAndFormat(prefixes, ev, kind, side, safeActorName, safeTargetName, minute, homeGoals, awayGoals, meta)
            ?: return factualFallback(kind, side, safeActorName)

        val extraClause = selectContextualFollowUp(
            kind = kind,
            side = side,
            minute = minute,
            gameSecond = gameSecond,
            homeGoals = homeGoals,
            awayGoals = awayGoals,
            ev = ev,
            actorName = safeActorName,
            targetName = safeTargetName,
            meta = meta
        )
        return if (extraClause != null) "$mainLine $extraClause" else mainLine
    }

    private fun selectPrefixes(
        kind: LabEventType,
        side: LabTeamSide?,
        meta: JSONObject?,
        minute: Int,
        homeGoals: Int,
        awayGoals: Int,
        wasShotOrRebound: Boolean
    ): List<String> {
        // A difference from the previous event's X is NOT proof of a switch
        // of play; the original pass event must explicitly say so.
        val isSwitchPlay = meta?.optString("passType") == "SWITCH"

        return when (kind) {
            LabEventType.MATCH_STARTED -> listOf("cv3_match_phase")
            LabEventType.KICKOFF -> {
                // A restart after a goal is not another match opening.
                if (minute <= 2 && homeGoals == 0 && awayGoals == 0) listOf("cv3_match_phase")
                else emptyList()
            }
            LabEventType.TACTICAL_PHASE_CHANGED -> {
                when {
                    tracker.isTenMen(side) && meta?.optString("tenMen") == "true" -> listOf("cv3_ten_men", "cv3_match_phase")
                    meta?.optString("highDefensiveLine") == "true" -> listOf("cv3_high_line", "cv3_match_phase")
                    else -> listOf("cv3_match_phase")
                }
            }
            LabEventType.POSSESSION_STARTED -> listOf("cv3_possession", "cv2_possession")
            LabEventType.PRESS -> {
                if (minute >= 72) listOf("cv3_press", "cv3_fatigue") else listOf("cv3_press")
            }
            // Winning possession alone doesn't prove a dangerous counterattack.
            LabEventType.TURNOVER -> emptyList()
            LabEventType.TRANSITION_STARTED -> listOf("cv3_transition")
            LabEventType.PASS_COMPLETED -> {
                val gain = meta?.optString("progressGain")?.toIntOrNull() ?: 0
                when {
                    meta?.optString("escapedPressure") == "true" -> listOf("cv3_escape_press", "cv3_build_up", "cv2_build_up")
                    isSwitchPlay -> listOf("cv3_switch_play")
                    meta?.optString("overload") == "true" -> listOf("cv3_overload", "cv3_build_up")
                    meta?.optString("sustainedPressure") == "true" -> listOf("cv3_sustained_pressure", "cv3_build_up")
                    gain >= 12 -> listOf("cv3_progressive_pass", "cv2_direction")
                    else -> listOf("cv3_build_up", "cv2_build_up")
                }
            }
            // An incomplete PASS doesn't establish a dribble, tackle or turnover.
            LabEventType.PASS_FAILED -> emptyList()
            LabEventType.THROUGH_BALL -> listOf("cv3_through_ball")
            LabEventType.FIRST_TOUCH -> {
                listOf("cv3_first_touch")
            }
            LabEventType.CARRY, LabEventType.DRIBBLE_STARTED -> {
                listOf("cv3_dribble", "cv2_dribble")
            }
            LabEventType.DRIBBLE_WON -> {
                listOf("cv3_dribble", "cv2_dribble")
            }
            LabEventType.DRIBBLE_LOST -> listOf("cv3_ball_loss", "cv2_ball_loss")
            LabEventType.CROSS -> listOf("cv3_cross", "cv2_cross")
            LabEventType.TACKLE_ATTEMPTED, LabEventType.TACKLE_WON,
            LabEventType.TACKLE_LOST -> emptyList()
            LabEventType.INTERCEPTION -> listOf("cv3_interception", "cv2_interception")
            LabEventType.BLOCK -> listOf("cv3_block")
            LabEventType.CLEARANCE -> listOf("cv3_clearance")
            LabEventType.GROUND_DUEL, LabEventType.AERIAL_DUEL -> listOf("cv3_duel")
            LabEventType.LOOSE_BALL -> listOf("cv3_loose_ball")
            LabEventType.SECOND_BALL_WON -> emptyList()
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
                    else -> listOf("cv3_shot", "cv2_shot")
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
            LabEventType.GOALKEEPER_CLAIM, LabEventType.GOALKEEPER_RUSH -> listOf("cv3_goalkeeper_action")
            LabEventType.GOALKEEPER_ASSIGNMENT_REQUIRED, LabEventType.GOALKEEPER_REASSIGNED -> emptyList()
            LabEventType.FOUL -> listOf("cv3_foul")
            LabEventType.ADVANTAGE_PLAYED -> listOf("cv3_advantage")
            LabEventType.YELLOW_CARD -> listOf("cv3_yellow_card", "cv2_yellow_card")
            LabEventType.RED_CARD -> {
                if (meta?.optString("secondYellow") == "true") listOf("cv3_second_yellow", "cv2_second_yellow")
                else listOf("cv3_red_card", "cv2_red_card")
            }
            LabEventType.OFFSIDE -> listOf("cv3_offside", "cv2_offside")
            LabEventType.INJURY -> listOf("cv3_injury", "cv2_injury")
            LabEventType.THROW_IN -> {
                if (meta?.optString("timeWasting") == "true") listOf("cv3_time_wasting", "cv3_throw_in")
                else listOf("cv3_throw_in")
            }
            LabEventType.GOAL_KICK -> {
                if (meta?.optString("timeWasting") == "true") listOf("cv3_time_wasting", "cv3_goal_kick")
                else listOf("cv3_goal_kick")
            }
            LabEventType.CORNER -> listOf("cv3_corner", "cv2_corner")
            LabEventType.FREE_KICK -> {
                if (meta?.optString("timeWasting") == "true") listOf("cv3_time_wasting", "cv3_free_kick")
                else listOf("cv3_free_kick", "cv2_free_kick")
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

        // Do not invent tactics, deliberate time-wasting or dominance from a
        // routine possession/pass merely because the match clock advanced.
        if (minute >= 20 && tracker.shouldAppendScore(gameSecond)) {
            return pickAndFormat(listOf("cv3_score", "cv2_score"), ev, kind, side, actorName, targetName, minute, homeGoals, awayGoals, meta, salt = 97L)
        }
        return null
    }

    /** Original catalogue text is retained. These fact-only fallback lines have their
     * own Android locale resources, so no Vietnamese is injected into English TTS. */
    private fun factualText(key: String, tokens: Map<String, String> = emptyMap()): String? {
        val id = context.resources.getIdentifier(key, "string", context.packageName)
        if (id == 0) return null
        var line = context.getString(id)
        for ((name, value) in tokens) {
            if (value.isBlank()) return null
            line = line.replace("{$name}", value)
        }
        return line.trim().takeIf { it.isNotBlank() && !it.contains('{') }
    }

    /** Exact-event fallback when the legacy catalogue has no proven variant. */
    private fun factualFallback(kind: LabEventType, side: LabTeamSide?, actor: String): String? {
        val team = teamName(side)
        return when (kind) {
            LabEventType.PASS_FAILED -> if (actor.isNotBlank()) factualText(
                "lab_commentary_pass_failed_player", mapOf("PLAYER" to actor)
            ) else factualText("lab_commentary_pass_failed_generic")
            LabEventType.TURNOVER -> factualText("lab_commentary_turnover", mapOf("TEAM" to team))
            LabEventType.SECOND_BALL_WON -> if (actor.isNotBlank()) factualText(
                "lab_commentary_second_ball_player", mapOf("PLAYER" to actor, "TEAM" to team)
            ) else factualText("lab_commentary_second_ball_team", mapOf("TEAM" to team))
            LabEventType.TACKLE_ATTEMPTED -> factualText("lab_commentary_tackle_attempted", mapOf("PLAYER" to actor))
            LabEventType.TACKLE_WON -> factualText("lab_commentary_tackle_won", mapOf("PLAYER" to actor))
            LabEventType.TACKLE_LOST -> factualText("lab_commentary_tackle_lost", mapOf("PLAYER" to actor))
            LabEventType.GOALKEEPER_ASSIGNMENT_REQUIRED -> factualText(
                "lab_commentary_goalkeeper_assignment_required", mapOf("TEAM" to team)
            )
            LabEventType.GOALKEEPER_REASSIGNED -> factualText(
                "lab_commentary_goalkeeper_reassigned", mapOf("TEAM" to team)
            )
            LabEventType.GOALKEEPER_DISTRIBUTION -> factualText(
                "lab_commentary_goalkeeper_distribution", mapOf("PLAYER" to actor)
            )
            LabEventType.TRANSITION_STARTED -> factualText("lab_commentary_transition_started", mapOf("TEAM" to team))
            else -> null
        }
    }

    private fun teamName(side: LabTeamSide?): String = when (side) {
        LabTeamSide.HOME -> homeTeam
        LabTeamSide.AWAY -> awayTeam
        null -> ""
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
        // FIRST_TOUCH events place the receiver in actor, while PASS_COMPLETED uses target.
        // Never silently substitute the opposing team for a missing receiver.
        val receiverName = if (kind == LabEventType.FIRST_TOUCH) actorName else targetName
        val shootoutForWinner = meta?.optString("shootout")?.split("-")
        val shootoutHome = shootoutForWinner?.getOrNull(0)?.toIntOrNull()
        val shootoutAway = shootoutForWinner?.getOrNull(1)?.toIntOrNull()
        val winnerIsKnown = homeGoals != awayGoals ||
            (shootoutHome != null && shootoutAway != null && shootoutHome != shootoutAway)
        val candidates = templatesForAny(prefixes).filter { (key, template) ->
            val policyAllows = LabCommentaryTemplatePolicy.allows(
                key, template, kind, minute, meta, actorName, targetName, receiverName, side
            )
            val phaseLine = if (key.startsWith("cv3_match_phase_")) {
                key.removePrefix("cv3_match_phase_").toIntOrNull()
            } else null
            val phaseValid = when {
                phaseLine == null -> true
                kind == LabEventType.MATCH_STARTED -> phaseLine in 1..3
                kind == LabEventType.KICKOFF -> phaseLine == 4
                kind == LabEventType.TACTICAL_PHASE_CHANGED -> phaseLine in 5..10
                else -> false
            }
            policyAllows && phaseValid && (!template.contains("{RECEIVER}") || receiverName.isNotBlank()) &&
                (!template.contains("{WINNER}") && !template.contains("{LOSER}") || winnerIsKnown)
        }
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
            meta?.has("shootout") == true && penHome.toIntOrNull() != penAway.toIntOrNull() ->
                if ((penHome.toIntOrNull() ?: 0) > (penAway.toIntOrNull() ?: 0)) homeTeam else awayTeam
            else -> ""
        }
        val loser = if (winner.isBlank()) "" else if (winner == homeTeam) awayTeam else homeTeam

        // Never substitute the name of a club when the missing token expects
        // an actual footballer (a key difference from the original Kotlin port).
        val safeActor = actorName
        val safeTarget = targetName
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

        val scored = meta?.optString("scored") == "true" ||
            meta?.optString("outcome")?.equals("SCORED", ignoreCase = true) == true
        val shootoutResult = if (kind == LabEventType.PENALTY_SHOOTOUT_KICK) {
            if (scored) "ghi bàn" else "không thành công"
        } else ""
        val values = mapOf(
            "PLAYER" to safeActor,
            "PASSER" to safeActor,
            "SHOOTER" to safeActor,
            "DEFENDER" to safeActor,
            "GOALKEEPER" to safeActor,
            "RECEIVER" to receiverName,
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
            "SHOOTOUT_RESULT" to shootoutResult,
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
