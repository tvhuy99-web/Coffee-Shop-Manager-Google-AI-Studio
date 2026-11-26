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

// --- V4.0 Protocol & Presentation Enums / Data Models ---

enum class LabSimulationMode { FULL_EVENT, FAST_BATCH }

enum class LabHighlightTier { NONE, EXTENDED, GOAL_KEY }

/**
 * Presentation modes selected in the independent "Cài đặt mô phỏng" menu.
 * Filtering runs purely on Kotlin presentation using Rust's `highlightTier`
 * without altering the 90-minute simulation outcome.
 */
enum class LabEngineVersion(val labelVi: String) {
    V4("4.0")
}

enum class LabViewingMode(val labelVi: String) {
    FULL_MATCH("Full trận"),
    EXTENDED_HIGHLIGHTS("Chi tiết"),
    SHORT_HIGHLIGHTS("Ngắn gọn")
}

enum class LabPreferredFoot { RIGHT, LEFT, EITHER }

enum class LabWeatherCondition { CLEAR, LIGHT_RAIN, HEAVY_RAIN, WINDY, HOT }

enum class LabPitchQuality { EXCELLENT, GOOD, HEAVY, POOR }

data class LabMatchEvent(
    val gameSecond: Int,
    val period: Int,
    val kind: LabEventType,
    val periodElapsedSecond: Int = gameSecond,
    val highlightTier: LabHighlightTier = LabHighlightTier.NONE,
    val highlightSequenceId: Int? = null,
    val side: LabTeamSide? = null,
    val actor: String? = null,
    val target: String? = null,
    val ballX: Float = 0f,
    val ballY: Float = 0f,
    val ballZ: Float = 0f,
    val metadata: Map<String, String> = emptyMap(),
    val simTimeMs: Long = gameSecond.toLong() * 1000L,
    val periodElapsedMs: Long = periodElapsedSecond.toLong() * 1000L,
    val dtMs: Long = 0L,
    val sequence: Int = 0
)

// --- V4.1 DES Playback, Narration & Checkpoint Shared Models ---

enum class DesHalfStage {
    PRE_MATCH,
    IN_HALF,
    HALF_TIME_BREAK,
    EXTRA_TIME_BREAK,
    SHOOTOUT,
    FULL_TIME;

    companion object {
        fun fromRustStage(raw: String?): DesHalfStage = when (raw?.trim()?.uppercase(Locale.ROOT)) {
            "PRE_MATCH" -> PRE_MATCH
            "IN_HALF" -> IN_HALF
            "HALF_TIME_BREAK" -> HALF_TIME_BREAK
            "EXTRA_TIME_BREAK" -> EXTRA_TIME_BREAK
            "PENALTY_SHOOTOUT", "SHOOTOUT" -> SHOOTOUT
            "FULL_TIME" -> FULL_TIME
            else -> PRE_MATCH
        }
    }
}

data class DesClockSnapshot(
    val simTimeMs: Long,
    val period: Int,
    val periodElapsedMs: Long,
    val ballInPlayMs: Long,
    val ballOutOfPlayMs: Long,
    val compensableLostMs: Long,
    val announcedAddedMs: Long,
    val additionalAddedMs: Long,
    val effectiveAddedMs: Long
)

data class PendingCommandStatus(
    val commandId: Long,
    val kind: String,
    val side: String,
    val status: String,
    val acceptedSimTimeMs: Long,
    val scheduledEffectiveSimTimeMs: Long,
    val actualEffectiveSimTimeMs: Long?
)

enum class CuePriority {
    P0_CRITICAL,
    P1_HIGH,
    P2_MEDIUM,
    P3_AMBIENT
}

enum class CueState {
    PLANNED,
    SYNTHESIZING,
    READY,
    PLAYING,
    COMPLETED,
    BACKEND_FAILED,
    CANCELLED
}

data class CommentaryCue(
    val cueId: String,
    val eventIds: List<Int>,
    val text: String,
    val anchorSimTimeMs: Long,
    var durationAudioMs: Long,
    val priority: CuePriority,
    val generationToken: Long,
    var state: CueState = CueState.PLANNED,
    var wavFilePath: String? = null
)

enum class TtsHealthStatus {
    HEALTHY,
    DEGRADED_BACKLOG,
    BACKEND_ERROR,
    UNAVAILABLE
}

data class PlaybackCheckpointState(
    val playbackSimTimeMs: Long,
    val committedRustSimTimeMs: Long,
    val playbackRate: Float,
    val paused: Boolean,
    val viewingMode: String,
    val includeTurningPointsInShort: Boolean,
    val lastPresentedEventSeq: Int,
    val lastCompletedCueId: String?,
    val inProgressCueId: String?,
    val inProgressCueAnchorSimMs: Long?,
    val inProgressCueText: String?,
    val inProgressCueOffsetMs: Long,
    val crowdState: String,
    val lastTransitionSimTimeMs: Long,
    val lastCrowdRiseSimTimeMs: Long,
    /** Buffered Rust events in (playbackSimTimeMs, committedRustSimTimeMs] not yet presented (CRITICAL-04). */
    val unpresentedEventsJson: String = "[]",
    /** Locked 1.0x unplayed commentary cues for seamless post-kill-process resume (CRITICAL-04 / QA-09). */
    val pendingCuesJson: String = "[]",
    val presentationStateJson: String = "{}"
)

data class LabMatchEnvironment(
    val weather: LabWeatherCondition = LabWeatherCondition.CLEAR,
    val pitchQuality: LabPitchQuality = LabPitchQuality.GOOD,
    val windStrength: Int = 15,
    val homeCrowdIntensity: Int = 60,
    val matchImportance: Int = 50
)

data class LabRefereeProfile(
    val strictness: Int = 50,
    val cardThreshold: Int = 50,
    val advantageTendency: Int = 50
)

/**
 * Full 54-attribute native V4 player profile on 1..20 scale,
 * matching `england_v2` `simulation.attributes` schema directly.
 */
data class LabPlayerAttributes54(
    // Technical (13)
    val finishing: Int,
    val passing: Int,
    val dribbling: Int,
    val technique: Int,
    val firstTouch: Int,
    val crossing: Int,
    val heading: Int,
    val longShots: Int,
    val tackling: Int,
    val marking: Int,
    val freeKicks: Int,
    val penalties: Int,
    val corners: Int,
    // Mental (14)
    val vision: Int,
    val decisions: Int,
    val composure: Int,
    val concentration: Int,
    val offTheBall: Int,
    val positioning: Int,
    val anticipation: Int,
    val teamwork: Int,
    val determination: Int,
    val workRate: Int,
    val leadership: Int,
    val flair: Int,
    val aggression: Int,
    val bravery: Int,
    // Physical (8)
    val pace: Int,
    val acceleration: Int,
    val strength: Int,
    val stamina: Int,
    val balance: Int,
    val agility: Int,
    val jumpingReach: Int,
    val naturalFitness: Int,
    // Goalkeeping (9)
    val reflexes: Int,
    val handling: Int,
    val oneOnOnes: Int,
    val commandOfArea: Int,
    val aerialReach: Int,
    val kicking: Int,
    val throwing: Int,
    val rushingOut: Int,
    val punching: Int,
    // Hidden (10: 6 in-match + 4 meta/career)
    val professionalism: Int,
    val ambition: Int,
    val consistency: Int,
    val bigMatches: Int,
    val adaptability: Int,
    val loyalty: Int,
    val pressure: Int,
    val injuryProneness: Int,
    val temperament: Int,
    val sportsmanship: Int
)

data class LabPlayerSnapshotV4(
    val playerId: String,
    val displayName: String,
    val attributes: LabPlayerAttributes54,
    val condition: Int = 100,
    val morale: Int = 65,
    val form: Int = 55,
    val age: Int = 24,
    val isInjured: Boolean = false,
    val isEligible: Boolean = true,
    val primaryPosition: String,
    val secondaryPosition: String = "",
    val preferredFoot: LabPreferredFoot = LabPreferredFoot.RIGHT,
    val weakFoot: Int = 3,
    val heightCm: Int = 180
)

// Lineup-planning attribute model

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
