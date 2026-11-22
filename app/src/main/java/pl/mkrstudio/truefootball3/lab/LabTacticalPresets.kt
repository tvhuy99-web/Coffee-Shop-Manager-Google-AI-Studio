package pl.mkrstudio.truefootball3.lab

import kotlin.math.abs
import kotlin.math.roundToInt

object LabTacticalPresets {
    enum class PlayerProfile(val labelVi: String) {
        GOALKEEPER("Thủ môn"),
        DEFENSIVE_ANCHOR("Mỏ neo phòng ngự"),
        WIDE_DEFENDER("Hậu vệ biên"),
        CENTRAL_CONTROLLER("Tiền vệ điều phối"),
        CENTRAL_CREATOR("Tiền vệ sáng tạo"),
        WIDE_ATTACKER("Tiền đạo cánh"),
        MOBILE_ATTACKER("Tiền đạo cơ động"),
        PHYSICAL_TARGET("Tiền đạo làm tường"),
        BALANCED("Cân bằng")
    }

    data class SlotCoord(
        val slotId: String,
        val outX: Int,
        val outY: Int,
        val inX: Int,
        val inY: Int
    ) {
            }

    data class FormationPreset(
        val id: String,
        val label: String,
        val slots: List<SlotCoord>
    )

    data class StylePreset(
        val id: String,
        val label: String,
        val pressing: Int,
        val tempo: Int,
        val lineHeight: Int,
        val width: Int,
        val timeWasting: Int,
        val offsideTrap: Boolean,
        val aggression: Int,
        val transitionRisk: Int,
        val buildUpRisk: Int,
        val concentration: Int = 50
    )

    data class TacticalConfig(
        val formation: FormationPreset = FORMATIONS[0],
        val style: StylePreset = STYLES[0]
    )

    data class PlannedLineup(
        val starters: List<LabPlayerSnapshot>,
        val bench: List<LabPlayerSnapshot>,
        val assignments: List<LabLineupAssignment>,
        val designatedGoalkeeperId: String,
        val suitabilityBySlot: Map<String, Int>,
        val setPiecePlan: LabSetPiecePlan
    )

    val FORMATIONS: List<FormationPreset> = listOf(
        FormationPreset(
            id = "433-v3",
            label = "4-3-3",
            slots = listOf(
                SlotCoord("GK", 50, 5, 50, 8),
                SlotCoord("LB", 16, 23, 16, 42),
                SlotCoord("LCB", 38, 22, 38, 31),
                SlotCoord("RCB", 62, 22, 62, 31),
                SlotCoord("RB", 84, 23, 84, 42),
                SlotCoord("DM", 50, 39, 50, 49),
                SlotCoord("LCM", 36, 51, 36, 61),
                SlotCoord("RCM", 64, 51, 64, 61),
                SlotCoord("LW", 12, 66, 18, 79),
                SlotCoord("RW", 88, 66, 82, 79),
                SlotCoord("CF", 50, 80, 50, 86)
            )
        ),
        FormationPreset(
            id = "442-v3",
            label = "4-4-2",
            slots = listOf(
                SlotCoord("GK", 50, 5, 50, 8),
                SlotCoord("LB", 16, 23, 16, 40),
                SlotCoord("LCB", 38, 22, 38, 31),
                SlotCoord("RCB", 62, 22, 62, 31),
                SlotCoord("RB", 84, 23, 84, 40),
                SlotCoord("LM", 17, 51, 17, 64),
                SlotCoord("LCM", 38, 50, 38, 60),
                SlotCoord("RCM", 62, 50, 62, 60),
                SlotCoord("RM", 83, 51, 83, 64),
                SlotCoord("LCF", 38, 79, 38, 86),
                SlotCoord("RCF", 62, 79, 62, 86)
            )
        ),
        FormationPreset(
            id = "4231-v3",
            label = "4-2-3-1",
            slots = listOf(
                SlotCoord("GK", 50, 5, 50, 8),
                SlotCoord("LB", 16, 23, 16, 42),
                SlotCoord("LCB", 38, 22, 38, 31),
                SlotCoord("RCB", 62, 22, 62, 31),
                SlotCoord("RB", 84, 23, 84, 42),
                SlotCoord("LDM", 39, 39, 39, 49),
                SlotCoord("RDM", 61, 39, 61, 49),
                SlotCoord("LW", 14, 65, 18, 79),
                SlotCoord("AM", 50, 65, 50, 76),
                SlotCoord("RW", 86, 65, 82, 79),
                SlotCoord("CF", 50, 80, 50, 88)
            )
        ),
        FormationPreset(
            id = "352-v3",
            label = "3-5-2",
            slots = listOf(
                SlotCoord("GK", 50, 5, 50, 8),
                SlotCoord("LCB", 31, 22, 31, 31),
                SlotCoord("CB", 50, 20, 50, 29),
                SlotCoord("RCB", 69, 22, 69, 31),
                SlotCoord("LWB", 12, 34, 12, 56),
                SlotCoord("DM", 50, 39, 50, 49),
                SlotCoord("LCM", 36, 51, 36, 62),
                SlotCoord("RCM", 64, 51, 64, 62),
                SlotCoord("RWB", 88, 34, 88, 56),
                SlotCoord("LCF", 38, 79, 38, 87),
                SlotCoord("RCF", 62, 79, 62, 87)
            )
        ),
        FormationPreset(
            id = "532-v3",
            label = "5-3-2",
            slots = listOf(
                SlotCoord("GK", 50, 5, 50, 8),
                SlotCoord("LWB", 14, 26, 14, 50),
                SlotCoord("LCB", 32, 21, 32, 30),
                SlotCoord("CB", 50, 19, 50, 27),
                SlotCoord("RCB", 68, 21, 68, 30),
                SlotCoord("RWB", 86, 26, 86, 50),
                SlotCoord("DM", 50, 39, 50, 48),
                SlotCoord("LCM", 36, 51, 36, 62),
                SlotCoord("RCM", 64, 51, 64, 62),
                SlotCoord("LCF", 38, 78, 38, 86),
                SlotCoord("RCF", 62, 78, 62, 86)
            )
        ),
        FormationPreset(
            id = "4141-v3",
            label = "4-1-4-1",
            slots = listOf(
                SlotCoord("GK", 50, 5, 50, 8),
                SlotCoord("LB", 16, 23, 16, 40),
                SlotCoord("LCB", 38, 22, 38, 31),
                SlotCoord("RCB", 62, 22, 62, 31),
                SlotCoord("RB", 84, 23, 84, 40),
                SlotCoord("DM", 50, 38, 50, 48),
                SlotCoord("LM", 16, 52, 16, 68),
                SlotCoord("LCM", 38, 52, 38, 64),
                SlotCoord("RCM", 62, 52, 62, 64),
                SlotCoord("RM", 84, 52, 84, 68),
                SlotCoord("CF", 50, 80, 50, 88)
            )
        ),
        FormationPreset(
            id = "343-v3",
            label = "3-4-3",
            slots = listOf(
                SlotCoord("GK", 50, 5, 50, 8),
                SlotCoord("LCB", 31, 22, 31, 31),
                SlotCoord("CB", 50, 20, 50, 29),
                SlotCoord("RCB", 69, 22, 69, 31),
                SlotCoord("LWB", 12, 38, 12, 58),
                SlotCoord("LCM", 38, 48, 38, 58),
                SlotCoord("RCM", 62, 48, 62, 58),
                SlotCoord("RWB", 88, 38, 88, 58),
                SlotCoord("LW", 16, 68, 20, 81),
                SlotCoord("CF", 50, 80, 50, 88),
                SlotCoord("RW", 84, 68, 80, 81)
            )
        ),
        FormationPreset(
            id = "442d-v3",
            label = "4-4-2 Kim cương",
            slots = listOf(
                SlotCoord("GK", 50, 5, 50, 8),
                SlotCoord("LB", 15, 24, 14, 46),
                SlotCoord("LCB", 38, 22, 38, 31),
                SlotCoord("RCB", 62, 22, 62, 31),
                SlotCoord("RB", 85, 24, 86, 46),
                SlotCoord("DM", 50, 38, 50, 48),
                SlotCoord("LCM", 34, 52, 34, 62),
                SlotCoord("RCM", 66, 52, 66, 62),
                SlotCoord("AM", 50, 64, 50, 75),
                SlotCoord("LCF", 38, 79, 38, 87),
                SlotCoord("RCF", 62, 79, 62, 87)
            )
        )
    )

    val STYLES: List<StylePreset> = listOf(
        StylePreset("balanced", "Cân bằng", 50, 50, 50, 50, 10, false, 50, 50, 50),
        StylePreset("high_press", "Pressing tầm cao", 82, 72, 74, 62, 5, true, 68, 68, 60),
        StylePreset("possession", "Kiểm soát bóng", 58, 42, 64, 64, 15, true, 45, 42, 38),
        StylePreset("counter", "Phản công nhanh", 42, 76, 36, 56, 15, false, 58, 82, 64),
        StylePreset("low_block", "Phòng ngự lùi sâu", 30, 40, 26, 38, 35, false, 62, 58, 40)
    )

    fun formationById(id: String?): FormationPreset =
        FORMATIONS.firstOrNull { it.id == id } ?: FORMATIONS[0]

    fun styleById(id: String?): StylePreset =
        STYLES.firstOrNull { it.id == id } ?: STYLES[0]

    fun defaultSlotInstructions(slot: SlotCoord): LabSlotInstructions {
        val isGk = slot.outY <= 12 && slot.outX in 25..75
        val wide = slot.outX <= 24 || slot.outX >= 76
        val fwdMove = if (isGk) 12 else (35 + (slot.inY - slot.outY) * 2).coerceIn(20, 88)
        val passRisk = when {
            isGk -> 25
            slot.inY >= 65 -> 64
            slot.inY >= 45 -> 56
            else -> 44
        }
        val crossing = if (isGk) 10 else if (wide) 72 else 34
        val dribbling = when {
            isGk -> 10
            wide && slot.inY >= 60 -> 72
            slot.inY >= 70 -> 66
            else -> 48
        }
        val defAgg = if (isGk) 20 else if (slot.outY <= 42) 64 else 50
        val support = if (isGk) 30 else (55 - abs(slot.inY - 50) / 3).coerceIn(35, 65)
        val roaming = if (isGk) 5 else if (slot.inY >= 65 && !wide) 55 else 35
        val shooting = when {
            isGk -> 5
            slot.inY >= 78 -> 78
            slot.inY >= 60 -> 60
            else -> 32
        }
        return LabSlotInstructions(
            slot.slotId, fwdMove, passRisk, crossing, dribbling, defAgg, support, roaming, shooting
        )
    }

    fun profileScore(a: LabPlayerAttributes, profile: PlayerProfile): Int = when (profile) {
        PlayerProfile.GOALKEEPER -> weighted(
            intArrayOf(a.goalkeeping, a.agility, a.decisions, a.composure, a.passing),
            intArrayOf(55, 14, 12, 10, 9)
        )
        PlayerProfile.DEFENSIVE_ANCHOR -> weighted(
            intArrayOf(a.defensivePositioning, a.tackling, a.decisions, a.strength, a.heading, a.workRate),
            intArrayOf(24, 22, 17, 13, 12, 12)
        )
        PlayerProfile.WIDE_DEFENDER -> weighted(
            intArrayOf(a.defensivePositioning, a.tackling, a.pace, a.stamina, a.workRate, a.crossing),
            intArrayOf(19, 17, 18, 17, 15, 14)
        )
        PlayerProfile.CENTRAL_CONTROLLER -> weighted(
            intArrayOf(a.passing, a.decisions, a.technique, a.composure, a.stamina, a.defensivePositioning),
            intArrayOf(22, 19, 17, 16, 14, 12)
        )
        PlayerProfile.CENTRAL_CREATOR -> weighted(
            intArrayOf(a.vision, a.passing, a.technique, a.decisions, a.composure, a.dribbling),
            intArrayOf(24, 20, 18, 16, 12, 10)
        )
        PlayerProfile.WIDE_ATTACKER -> weighted(
            intArrayOf(a.pace, a.dribbling, a.crossing, a.offBallMovement, a.technique, a.stamina),
            intArrayOf(22, 21, 17, 16, 13, 11)
        )
        PlayerProfile.MOBILE_ATTACKER -> weighted(
            intArrayOf(a.shooting, a.offBallMovement, a.composure, a.pace, a.technique, a.dribbling),
            intArrayOf(24, 22, 18, 14, 12, 10)
        )
        PlayerProfile.PHYSICAL_TARGET -> weighted(
            intArrayOf(a.shooting, a.heading, a.strength, a.offBallMovement, a.composure, a.technique),
            intArrayOf(22, 21, 19, 15, 13, 10)
        )
        PlayerProfile.BALANCED -> weighted(
            intArrayOf(a.passing, a.technique, a.decisions, a.pace, a.stamina, a.workRate),
            intArrayOf(18, 18, 18, 15, 15, 16)
        )
    }

    private fun slotProfile(slot: SlotCoord): PlayerProfile {
        if (slot.outY <= 12 && slot.outX in 25..75) return PlayerProfile.GOALKEEPER
        val wide = slot.outX <= 24 || slot.outX >= 76
        val meanY = (slot.outY + slot.inY) / 2
        return when {
            meanY <= 36 && wide -> PlayerProfile.WIDE_DEFENDER
            meanY <= 36 -> PlayerProfile.DEFENSIVE_ANCHOR
            meanY <= 66 && wide -> PlayerProfile.WIDE_ATTACKER
            meanY <= 56 -> PlayerProfile.CENTRAL_CONTROLLER
            meanY <= 72 && !wide -> PlayerProfile.CENTRAL_CREATOR
            wide -> PlayerProfile.WIDE_ATTACKER
            else -> PlayerProfile.MOBILE_ATTACKER
        }
    }

    private fun positionCompatibilityBonus(rawPos: String?, slot: SlotCoord): Double {
        val pos = rawPos?.trim()?.uppercase().orEmpty()
        val wideLeft = slot.outX <= 30
        val wideRight = slot.outX >= 70
        val central = !wideLeft && !wideRight
        val y = (slot.outY + slot.inY) / 2
        return when (pos) {
            "GK" -> if (slot.outY <= 12) 14.0 else -45.0
            "SW", "CB", "B", "DF", "D" -> if (y <= 38 && central) 10.0 else if (y <= 42) 3.0 else -12.0
            "LB", "LWB" -> if (wideLeft && y <= 52) 10.0 else if (wideRight && y <= 46) 3.0 else -8.0
            "RB", "RWB" -> if (wideRight && y <= 52) 10.0 else if (wideLeft && y <= 46) 3.0 else -8.0
            "DM", "CDM" -> if (central && y in 32..58) 10.0 else if (central && y <= 65) 4.0 else -7.0
            "CM", "M", "MF" -> if (central && y in 42..68) 10.0 else if (y in 35..74) 4.0 else -6.0
            "LM" -> if (wideLeft && y in 42..75) 10.0 else if (wideRight && y in 45..75) 3.0 else -7.0
            "RM" -> if (wideRight && y in 42..75) 10.0 else if (wideLeft && y in 45..75) 3.0 else -7.0
            "AM", "CAM" -> if (central && y in 55..80) 10.0 else if (y in 48..85) 4.0 else -7.0
            "LW" -> if (wideLeft && y >= 58) 10.0 else if (wideRight && y >= 62) 5.0 else -6.0
            "RW" -> if (wideRight && y >= 58) 10.0 else if (wideLeft && y >= 62) 5.0 else -6.0
            "CF", "F", "ST", "FW", "SS" -> if (y >= 70 && central) 10.0 else if (y >= 62) 5.0 else -10.0
            else -> 0.0
        }
    }

    fun playerSlotSuitability(player: LabPlayerSnapshot, slot: SlotCoord): Int {
        val base = profileScore(player.attributes, slotProfile(slot)).toDouble()
        var bonus = positionCompatibilityBonus(player.primaryPosition, slot)
        if (!player.secondaryPosition.isNullOrBlank()) {
            val sec = positionCompatibilityBonus(player.secondaryPosition, slot) * 0.72
            if (sec > bonus) bonus = sec
        }
        val readiness = (player.condition - 75.0) * 0.14 +
            (player.morale - 50.0) * 0.06 +
            (player.form - 50.0) * 0.08
        val penalty = if (player.isInjured || !player.isEligible) -40.0 else 0.0
        return (base + bonus + readiness + penalty).roundToInt().coerceIn(1, 100)
    }

    fun planOptimalLineup(
        squad: List<LabPlayerSnapshot>,
        formation: FormationPreset,
        benchSize: Int = 9
    ): PlannedLineup {
        val slots = formation.slots
        require(squad.size >= 11 && slots.size == 11) { "Invalid squad or formation size" }
        val gkSlotIdx = slots.indices.minByOrNull { slots[it].outY } ?: 0
        val n = squad.size
        val scores = Array(n) { i -> IntArray(11) { s -> playerSlotSuitability(squad[i], slots[s]) } }

        val used = HashSet<Int>()
        val slotToPlayer = IntArray(11) { -1 }
        val bestGk = (0 until n).maxWith(compareBy<Int> { scores[it][gkSlotIdx] }.thenBy { squad[it].attributes.goalkeeping })
        slotToPlayer[gkSlotIdx] = bestGk
        used.add(bestGk)

        val unassigned = (0 until 11).filter { it != gkSlotIdx }.toMutableList()
        while (unassigned.isNotEmpty()) {
            var chosenPos = 0
            var bestGap = Int.MIN_VALUE
            var chosenPlayer = 0
            for (pos in unassigned.indices) {
                val s = unassigned[pos]
                var top1Score = Int.MIN_VALUE
                var top1Idx = 0
                var top2Score = Int.MIN_VALUE
                for (i in 0 until n) {
                    if (used.contains(i)) continue
                    val sc = scores[i][s]
                    if (sc > top1Score) {
                        top2Score = top1Score
                        top1Score = sc
                        top1Idx = i
                    } else if (sc > top2Score) {
                        top2Score = sc
                    }
                }
                val gap = if (top2Score == Int.MIN_VALUE) top1Score else top1Score - top2Score
                if (gap > bestGap) {
                    bestGap = gap
                    chosenPos = pos
                    chosenPlayer = top1Idx
                }
            }
            val s = unassigned.removeAt(chosenPos)
            slotToPlayer[s] = chosenPlayer
            used.add(chosenPlayer)
        }

        var improved = true
        var passes = 0
        while (improved && passes++ < 12) {
            improved = false
            for (a in 0 until 11) {
                if (a == gkSlotIdx) continue
                for (b in (a + 1) until 11) {
                    if (b == gkSlotIdx) continue
                    val pa = slotToPlayer[a]
                    val pb = slotToPlayer[b]
                    if (scores[pa][b] + scores[pb][a] > scores[pa][a] + scores[pb][b]) {
                        slotToPlayer[a] = pb
                        slotToPlayer[b] = pa
                        improved = true
                    }
                }
                val pa = slotToPlayer[a]
                for (cand in 0 until n) {
                    if (used.contains(cand)) continue
                    if (scores[cand][a] > scores[pa][a]) {
                        used.remove(pa)
                        used.add(cand)
                        slotToPlayer[a] = cand
                        improved = true
                        break
                    }
                }
            }
        }

        val starters = ArrayList<LabPlayerSnapshot>(11)
        val assignments = ArrayList<LabLineupAssignment>(11)
        val suitabilityBySlot = LinkedHashMap<String, Int>(11)
        for (s in 0 until 11) {
            val p = squad[slotToPlayer[s]]
            starters.add(p)
            assignments.add(LabLineupAssignment(p.playerId, slots[s].slotId))
            suitabilityBySlot[slots[s].slotId] = scores[slotToPlayer[s]][s]
        }
        val gkId = squad[slotToPlayer[gkSlotIdx]].playerId

        val remaining = (0 until n).filter { !used.contains(it) }.toMutableList()
        val benchIndices = ArrayList<Int>(benchSize)
        for (line in 0..3) {
            if (benchIndices.size >= benchSize) break
            val bestInLine = remaining
                .filter { lineGroup(squad[it].primaryPosition) == line }
                .maxByOrNull { bestOverall(squad[it].attributes) }
            if (bestInLine != null) {
                remaining.remove(bestInLine)
                benchIndices.add(bestInLine)
            }
        }
        remaining.sortByDescending { bestOverall(squad[it].attributes) }
        for (idx in remaining) {
            if (benchIndices.size >= benchSize) break
            benchIndices.add(idx)
        }
        val bench = benchIndices.map { squad[it] }
        val setPiecePlan = buildAutoSetPiecePlan(starters, assignments, gkId)
        return PlannedLineup(starters, bench, assignments, gkId, suitabilityBySlot, setPiecePlan)
    }

    fun buildAutoSetPiecePlan(
        starters: List<LabPlayerSnapshot>,
        assignments: List<LabLineupAssignment>,
        gkId: String
    ): LabSetPiecePlan {
        val slotByPlayer = assignments.associate { it.playerId to it.slotId }
        val outfield = starters
            .filter { it.playerId != gkId }
            .mapNotNull { p -> slotByPlayer[p.playerId]?.let { sid -> p to sid } }

        fun topSlots(count: Int, score: (LabPlayerSnapshot) -> Int): List<String> =
            outfield.sortedWith(compareByDescending<Pair<LabPlayerSnapshot, String>> { score(it.first) }.thenBy { it.second })
                .take(count)
                .map { it.second }

        val penalties = topSlots(3) { p -> p.attributes.shooting * 4 + p.attributes.composure * 4 + p.attributes.decisions * 2 }
        val corners = topSlots(3) { p -> p.attributes.crossing * 4 + p.attributes.passing * 3 + p.attributes.technique * 2 + p.attributes.vision }
        val closeFk = topSlots(3) { p -> p.attributes.shooting * 4 + p.attributes.technique * 3 + p.attributes.composure * 2 + p.attributes.passing }
        val farFk = topSlots(3) { p -> p.attributes.crossing * 4 + p.attributes.passing * 3 + p.attributes.vision * 3 }
        val captains = topSlots(3) { p -> p.attributes.charisma * 5 + p.attributes.composure * 2 + p.attributes.decisions * 2 + p.age }

        val primaryCorner = corners.firstOrNull().orEmpty()
        val primaryFk = farFk.firstOrNull().orEmpty()
        fun boxTargets(exclude: String): List<String> =
            outfield.filter { it.second != exclude }
                .sortedWith(compareByDescending<Pair<LabPlayerSnapshot, String>> {
                    val a = it.first.attributes
                    a.heading * 4 + a.strength * 3 + a.offBallMovement * 2 + a.shooting
                }.thenBy { it.second })
                .take(5)
                .map { it.second }

        return LabSetPiecePlan(
            penalties,
            corners,
            closeFk,
            farFk,
            captains,
            boxTargets(primaryCorner),
            boxTargets(primaryFk),
            boxTargets(primaryFk)
        )
    }

    private fun bestOverall(a: LabPlayerAttributes): Int =
        PlayerProfile.values().maxOf { profileScore(a, it) }

    private fun lineGroup(rawPos: String?): Int = when (rawPos?.trim()?.uppercase().orEmpty()) {
        "GK" -> 0
        "SW", "CB", "LB", "RB", "LWB", "RWB", "B", "DF", "D" -> 1
        "DM", "CDM", "CM", "LM", "RM", "AM", "CAM", "M", "MF" -> 2
        else -> 3
    }

    private fun weighted(values: IntArray, weights: IntArray): Int {
        var sum = 0
        var total = 0
        for (i in values.indices) {
            sum += values[i] * weights[i]
            total += weights[i]
        }
        return if (total == 0) 0 else (sum.toFloat() / total.toFloat()).roundToInt().coerceIn(1, 99)
    }
}
