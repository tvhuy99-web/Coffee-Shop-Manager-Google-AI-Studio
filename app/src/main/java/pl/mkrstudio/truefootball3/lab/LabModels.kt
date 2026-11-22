package pl.mkrstudio.truefootball3.lab

import java.util.Locale
import java.util.Random

enum class LabTeamSide { HOME, AWAY }

enum class LabEventType {
    MATCH_STARTED,
    KICKOFF,
    POSSESSION_STARTED,
    PRESSURE_APPLIED,
    TACTICAL_PHASE_CHANGED,
    TRANSITION_STARTED,
    TRANSITION_ENDED,
    PASS_STARTED,
    PASS_COMPLETED,
    PASS_FAILED,
    THROUGH_BALL,
    CARRY,
    DRIBBLE_STARTED,
    DRIBBLE_WON,
    DRIBBLE_LOST,
    FIRST_TOUCH,
    CROSS,
    PRESS,
    TACKLE_ATTEMPTED,
    TACKLE_WON,
    TACKLE_LOST,
    INTERCEPTION,
    BLOCK,
    CLEARANCE,
    GROUND_DUEL,
    AERIAL_DUEL,
    LOOSE_BALL,
    SECOND_BALL_WON,
    REBOUND,
    TURNOVER,
    SHOT,
    SHOT_ON_TARGET,
    SHOT_OFF_TARGET,
    HEADER,
    GOAL,
    OWN_GOAL,
    SAVE,
    GOALKEEPER_CLAIM,
    GOALKEEPER_PUNCH,
    GOALKEEPER_RUSH,
    GOALKEEPER_DISTRIBUTION,
    FOUL,
    ADVANTAGE_PLAYED,
    YELLOW_CARD,
    RED_CARD,
    OFFSIDE,
    INJURY,
    THROW_IN,
    GOAL_KICK,
    CORNER,
    FREE_KICK,
    PENALTY_AWARDED,
    PENALTY_KICK,
    RESTART_SCHEDULED,
    SUBSTITUTION,
    GOALKEEPER_ASSIGNMENT_REQUIRED,
    GOALKEEPER_REASSIGNED,
    COMMAND_ACCEPTED,
    COMMAND_REJECTED,
    ADDED_TIME_ANNOUNCED,
    HALF_TIME,
    EXTRA_TIME_STARTED,
    EXTRA_TIME_HALF_TIME,
    PENALTY_SHOOTOUT_STARTED,
    PENALTY_SHOOTOUT_KICK,
    PENALTY_SHOOTOUT_ENDED,
    FULL_TIME
}

data class LabPlayerAttributes(
    val goalkeeping: Int,
    val tackling: Int,
    val passing: Int,
    val crossing: Int,
    val technique: Int,
    val dribbling: Int,
    val shooting: Int,
    val heading: Int,
    val pace: Int,
    val agility: Int,
    val strength: Int,
    val stamina: Int,
    val vision: Int,
    val decisions: Int,
    val composure: Int,
    val offBallMovement: Int,
    val defensivePositioning: Int,
    val workRate: Int,
    val charisma: Int,
    val temperament: Int
) {
    fun toIntArray(): IntArray = intArrayOf(
        goalkeeping, tackling, passing, crossing, technique,
        dribbling, shooting, heading, pace, agility,
        strength, stamina, vision, decisions, composure,
        offBallMovement, defensivePositioning, workRate, charisma, temperament
    )

    companion object {

        fun generateDeterministic(stableId: String, skill: Int, rawPosition: String?): LabPlayerAttributes {
            val base = skill.coerceIn(1, 99)
            val pos = rawPosition?.trim()?.uppercase(Locale.ROOT).orEmpty()
            var seed = -3750763034362895579L
            for (ch in "LAB|$stableId") {
                seed = seed xor ch.code.toLong()
                seed *= 1099511628211L
            }
            val rng = Random(seed)
            fun attr(bias: Int): Int = (base + bias + rng.nextInt(9) - 4).coerceIn(1, 99)

            val isGk = pos == "GK"
            val isCb = pos in setOf("CB", "SW", "B")
            val isFb = pos in setOf("LB", "RB", "LWB", "RWB")
            val isDm = pos in setOf("DM", "CDM")
            val isCm = pos in setOf("CM", "M")
            val isAm = pos in setOf("AM", "CAM")
            val isW = pos in setOf("LW", "RW", "LM", "RM")
            val isFw = pos in setOf("CF", "F", "ST", "FW")

            return LabPlayerAttributes(
                goalkeeping = if (isGk) attr(6) else (12 + rng.nextInt(14)),
                tackling = when {
                    isGk -> attr(-35)
                    isCb || isDm -> attr(7)
                    isFb -> attr(5)
                    isCm -> attr(1)
                    else -> attr(-12)
                },
                passing = when {
                    isCm || isAm || isDm -> attr(6)
                    isW || isFb -> attr(2)
                    isGk -> attr(-14)
                    else -> attr(-2)
                },
                crossing = when {
                    isW || isFb -> attr(7)
                    isAm || isCm -> attr(1)
                    isGk -> attr(-38)
                    else -> attr(-10)
                },
                technique = when {
                    isAm || isW || isFw -> attr(5)
                    isCm -> attr(3)
                    isGk -> attr(-20)
                    else -> attr(-4)
                },
                dribbling = when {
                    isW || isAm -> attr(7)
                    isFw -> attr(4)
                    isFb || isCm -> attr(1)
                    isGk -> attr(-38)
                    else -> attr(-10)
                },
                shooting = when {
                    isFw -> attr(8)
                    isW || isAm -> attr(4)
                    isCm -> attr(-2)
                    isGk -> attr(-42)
                    else -> attr(-15)
                },
                heading = when {
                    isCb || isFw -> attr(7)
                    isDm -> attr(2)
                    isGk -> attr(-28)
                    else -> attr(-6)
                },
                pace = when {
                    isW || isFb -> attr(7)
                    isFw -> attr(4)
                    isGk -> attr(-25)
                    else -> attr(-2)
                },
                agility = when {
                    isGk || isW || isAm -> attr(5)
                    isFw || isFb -> attr(3)
                    else -> attr(-2)
                },
                strength = when {
                    isCb || isDm -> attr(7)
                    isFw || isGk -> attr(3)
                    else -> attr(-3)
                },
                stamina = when {
                    isFb || isCm || isDm -> attr(5)
                    isGk -> attr(-15)
                    else -> attr(1)
                },
                vision = when {
                    isAm || isCm -> attr(7)
                    isDm || isW -> attr(3)
                    isGk -> attr(-15)
                    else -> attr(-4)
                },
                decisions = attr(2),
                composure = if (isFw || isGk || isCm) attr(4) else attr(1),
                offBallMovement = when {
                    isFw || isW || isAm -> attr(7)
                    isCm || isFb -> attr(2)
                    isGk -> attr(-35)
                    else -> attr(-6)
                },
                defensivePositioning = when {
                    isCb || isDm -> attr(8)
                    isFb -> attr(5)
                    isCm -> attr(1)
                    isGk -> attr(-12)
                    else -> attr(-14)
                },
                workRate = if (isFb || isDm || isCm) attr(5) else attr(1),
                charisma = attr(0),
                temperament = (55 + rng.nextInt(30)).coerceIn(1, 99)
            )
        }
    }
}

data class LabPlayerSnapshot(
    val playerId: String,
    val displayName: String,
    val attributes: LabPlayerAttributes,
    val condition: Int = 100,
    val morale: Int = 65,
    val form: Int = 55,
    val age: Int = 24,
    val isInjured: Boolean = false,
    val isEligible: Boolean = true,
    val primaryPosition: String,
    val secondaryPosition: String = ""
)

data class LabLineupAssignment(val playerId: String, val slotId: String)

data class LabSlotInstructions(
    val slotId: String,
    val forwardMovement: Int,
    val passingRisk: Int,
    val crossingTendency: Int,
    val dribblingTendency: Int,
    val defensiveAggression: Int,
    val supportDistance: Int,
    val roaming: Int,
    val shootingTendency: Int
)

data class LabSetPiecePlan(
    val penaltyTakerSlotIds: List<String>,
    val cornerTakerSlotIds: List<String>,
    val closeFreeKickTakerSlotIds: List<String>,
    val farFreeKickTakerSlotIds: List<String>,
    val captainSlotIds: List<String>,
    val cornerBoxSlotIds: List<String>,
    val closeFreeKickBoxSlotIds: List<String>,
    val farFreeKickBoxSlotIds: List<String>
)
