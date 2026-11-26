package pl.mkrstudio.truefootball3.match_commentary

import pl.mkrstudio.truefootball3.lab.*

import org.json.JSONObject

/**
 * Evidence gates for the ORIGINAL PingFootball XML catalogue.
 *
 * The source catalogue contains wonderful, but often extremely specific, lines.
 * A label such as BALL_LOSS or LOOSE_BALL cannot prove that a player dribbled,
 * an opponent intercepted, or that a counterattack began. This policy narrows
 * admissibility to what the incoming Rust event actually establishes.
 *
 * Never change XML content here; rejected phrases remain available if later
 * events/metadata supply the missing evidence.
 */
internal object LabCommentaryTemplatePolicy {
    private val tokens = Regex("\\{([A-Z][A-Z0-9_]*)\\}")

    private fun isOneOf(key: String, prefix: String, ids: Set<Int>): Boolean {
        if (!key.startsWith("${prefix}_")) return false
        return key.removePrefix("${prefix}_").toIntOrNull() in ids
    }

    private fun groupId(key: String, prefix: String): Int? =
        key.takeIf { it.startsWith("${prefix}_") }
            ?.removePrefix("${prefix}_")?.toIntOrNull()

    private fun ids(vararg indexes: Int): Set<Int> = indexes.toSet()

    /**
     * Gate individual templates, not just broad XML groups. Every accepted
     * sentence must be compatible with its event type and known metadata.
     */
    fun allows(
        key: String,
        template: String,
        kind: LabEventType,
        minute: Int,
        meta: JSONObject?,
        actorName: String,
        targetName: String,
        receiverName: String,
        side: LabTeamSide?
    ): Boolean {
        // Java V3's resolver rejects templates with unavailable player identity.
        val availablePlayer = actorName.isNotBlank() && actorName != "null"
        val availableTarget = targetName.isNotBlank() && targetName != "null"
        val availableReceiver = receiverName.isNotBlank() && receiverName != "null"
        if (kind == LabEventType.FIRST_TOUCH && !availableReceiver) return false
        val required = tokens.findAll(template).map { it.groupValues[1] }.toSet()
        if ((required intersect setOf("PLAYER", "PASSER", "SHOOTER", "DEFENDER", "GOALKEEPER", "PLAYER_OUT")).isNotEmpty() && !availablePlayer) return false
        if ((required intersect setOf("PRESSURED_PLAYER", "PLAYER_IN")).isNotEmpty() && !availableTarget) return false
        if ("RECEIVER" in required && !availableReceiver) return false
        if ("SHOOTOUT_RESULT" in required &&
            meta?.has("scored") != true &&
            meta?.optString("outcome")?.uppercase() !in setOf("SCORED", "MISSED")
        ) return false
        if ((required intersect setOf("TEAM", "OPPONENT", "ATTACKING_TEAM", "DEFENDING_TEAM" )).isNotEmpty() && side == null) return false

        groupId(key, "cv3_match_phase")?.let { number ->
            return when (kind) {
                LabEventType.MATCH_STARTED -> number in 1..2
                LabEventType.KICKOFF -> number == 4 && minute <= 2
                LabEventType.TACTICAL_PHASE_CHANGED -> when (number) {
                    5, 8 -> true  // time/score are the only facts required
                    6 -> minute >= 15
                    7, 9 -> minute >= 65
                    10 -> minute >= 75
                    else -> false
                }
                else -> false
            }
        }
        groupId(key, "cv3_score")?.let { number ->
            return kind == LabEventType.GOAL || number != 7
        }
        groupId(key, "cv2_score")?.let { number ->
            return kind == LabEventType.GOAL || number != 3
        }
        groupId(key, "cv3_loose_ball")?.let { number ->
            return kind == LabEventType.LOOSE_BALL && number in ids(1, 2, 3, 5, 6, 7, 8, 11, 12, 13, 14, 16, 17, 18, 19)
        }
        if (key.startsWith("cv2_chaos_")) return kind == LabEventType.REBOUND && meta?.optString("inBox") == "true"
        if (key.startsWith("cv3_ball_loss_") || key.startsWith("cv2_ball_loss_")) {
            // PASS_FAILED is not DRIBBLE_LOST or an interception.
            return kind == LabEventType.DRIBBLE_LOST &&
                (isOneOf(key,"cv3_ball_loss",ids(9,11,13,14,16,19,20,21)) ||
                 isOneOf(key,"cv2_ball_loss",ids(1,2,4,5,6,7)))
        }
        groupId(key,"cv2_build_up")?.let { number ->
            return kind == LabEventType.PASS_COMPLETED && number in ids(1,2,3,8)
        }
        groupId(key,"cv3_build_up")?.let { number ->
            return kind == LabEventType.PASS_COMPLETED && number in ids(10,13,14,25)
        }
        groupId(key,"cv3_progressive_pass")?.let { number ->
            val gain = meta?.optString("progressGain")?.toIntOrNull() ?: return false
            return kind == LabEventType.PASS_COMPLETED && gain >= 12 && number in ids(4,6,10,12)
        }
        groupId(key,"cv3_switch_play")?.let { number ->
            return kind == LabEventType.PASS_COMPLETED && meta?.optString("passType") == "SWITCH" && number in ids(1,3,7,10,11)
        }
        groupId(key,"cv3_dribble")?.let { number ->
            return when (kind) {
                LabEventType.DRIBBLE_WON -> number in ids(9,10,11,12,14,15,17,18,19,20,21,22)
                LabEventType.CARRY, LabEventType.DRIBBLE_STARTED -> number in ids(9,10,14,17,18,20,22)
                else -> false
            }
        }
        groupId(key,"cv2_dribble")?.let { number ->
            return when (kind) {
                LabEventType.DRIBBLE_WON -> number in ids(1,2,3,5,8)
                LabEventType.CARRY, LabEventType.DRIBBLE_STARTED -> number in ids(1,2,3,8)
                else -> false
            }
        }
        groupId(key,"cv3_through_ball")?.let { number ->
            return kind == LabEventType.THROUGH_BALL && number in ids(1,6,7,8,10,13)
        }
        groupId(key,"cv3_cross")?.let { number ->
            return kind == LabEventType.CROSS && number == 9
        }
        groupId(key,"cv2_cross")?.let { number ->
            return kind == LabEventType.CROSS && number in ids(1,3,4,7)
        }
        groupId(key,"cv3_press")?.let { number ->
            return kind == LabEventType.PRESS && number in ids(2,4,11,18,22,23,24)
        }
        groupId(key,"cv3_shot")?.let { number ->
            return kind == LabEventType.SHOT && number in ids(12,13,18)
        }
        groupId(key,"cv3_shot_on_target")?.let { number ->
            return kind == LabEventType.SHOT_ON_TARGET && number in ids(2,3,4,7,8)
        }
        groupId(key,"cv3_foul")?.let { number ->
            return kind == LabEventType.FOUL && number in ids(1,2,3,5,7,10)
        }
        groupId(key,"cv3_goalkeeper_punch")?.let { number ->
            return kind == LabEventType.GOALKEEPER_PUNCH && number in ids(1,2,6)
        }
        if (key.startsWith("cv2_keeper_reflect_")) return kind == LabEventType.SAVE
        groupId(key,"cv3_first_touch")?.let { number ->
            return kind == LabEventType.FIRST_TOUCH && number in ids(1,2,7,8,10,13,14,17,18,19,20)
        }
        groupId(key,"cv3_possession")?.let { number ->
            return kind == LabEventType.POSSESSION_STARTED && number in ids(12,20)
        }
        groupId(key,"cv2_possession")?.let { number ->
            return kind == LabEventType.POSSESSION_STARTED && number in ids(1,2,3,6,8)
        }
        groupId(key,"cv3_transition")?.let { number ->
            return kind == LabEventType.TRANSITION_STARTED && number in ids(5,9,20,21)
        }
        if (key.startsWith("cv2_counter_") || key.startsWith("cv3_counter_attack_")) {
            // A transition STARTED is not evidence of an already dangerous counter.
            return false
        }
        if (key.startsWith("cv3_defensive_recovery_")) {
            // Tackling success does not imply recovery to a defensive position.
            return false
        }
        groupId(key,"cv3_goalkeeper_distribution")?.let { number ->
            return kind == LabEventType.GOALKEEPER_DISTRIBUTION && number in ids(2,4)
        }
        groupId(key,"cv3_goalkeeper_action")?.let { number ->
            return when (kind) {
                LabEventType.GOALKEEPER_CLAIM -> number in ids(1,3,4,6,7,10,11,15,16)
                LabEventType.GOALKEEPER_RUSH -> number in ids(2,5,9)
                else -> false
            }
        }
        groupId(key,"cv3_duel")?.let { number ->
            return kind in setOf(LabEventType.GROUND_DUEL,LabEventType.AERIAL_DUEL) &&
                number in ids(1,2,4,5,8,11,12,14,15)
        }
        groupId(key,"cv3_missed")?.let { number ->
            return kind == LabEventType.SHOT_OFF_TARGET && number == 9
        }
        groupId(key,"cv2_missed")?.let { number ->
            return kind == LabEventType.SHOT_OFF_TARGET && number == 5
        }
        if (key.startsWith("cv3_escape_press_")) {
            // A preceding PRESS event cannot prove that the pressing was escaped.
            return meta?.optString("escapedPressure") == "true"
        }
        if (key.startsWith("cv3_overload_")) return meta?.optString("overload") == "true"
        if (key.startsWith("cv3_sustained_pressure_")) return meta?.optString("sustainedPressure") == "true"
        if (key.startsWith("cv3_time_wasting_")) return meta?.optString("timeWasting") == "true"
        if (key.startsWith("cv3_high_line_")) return meta?.optString("highDefensiveLine") == "true"
        if (key.startsWith("cv3_ten_men_")) return meta?.optString("tenMen") == "true"
        if (key.startsWith("cv2_free_kick_execution_")) return meta?.optString("freeKickExecuted") == "true"
        if (key.startsWith("cv2_technical_")) return meta?.optString("finesse") == "true" || meta?.optString("shotType") == "CURLED"
        if (key.startsWith("cv2_one_on_one_")) return meta?.optString("oneVsOne") == "true"
        if (key.startsWith("cv2_second_shot_")) return meta?.optString("reboundShot") == "true"
        if (key.startsWith("cv2_distance_shot_")) return (meta?.optString("distance")?.toDoubleOrNull() ?: 0.0) >= 22.0
        return true
    }
}
