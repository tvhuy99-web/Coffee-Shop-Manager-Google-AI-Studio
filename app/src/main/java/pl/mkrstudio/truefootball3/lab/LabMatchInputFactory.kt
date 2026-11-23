package pl.mkrstudio.truefootball3.lab

import android.content.Context
import org.json.JSONArray
import org.json.JSONObject

class LabMatchInputFactory(context: Context, assetPath: String = "england_v2/clubs.json") {
    private val appContext = context.applicationContext
    private val source: JSONObject = JSONObject(
        appContext.assets.open(assetPath).bufferedReader(Charsets.UTF_8).use { it.readText() }
    )
    private val playerNames = HashMap<String, String>()

    companion object {
        const val ENGINE_VERSION = "v3.17.0-realism-1"
    }

    fun name(playerId: String?): String {
        if (playerId.isNullOrEmpty()) return ""
        return playerNames[playerId] ?: playerId
    }

    fun describeLineupAndSetPieces(teamName: String, tactics: LabTacticalPresets.TacticalConfig): String {
        val squad = loadSquadSnapshots(teamName)
        val plan = LabTacticalPresets.planOptimalLineup(squad, tactics.formation, benchSize = 9)
        val playerBySlot = plan.assignments.associate { it.slotId to name(it.playerId) }
        val sp = plan.setPiecePlan
        return buildString {
            appendLine("=== $teamName (${tactics.formation.label} • ${tactics.style.label}) ===")
            for (slot in tactics.formation.slots) {
                val pName = playerBySlot[slot.slotId] ?: "-"
                val fit = plan.suitabilityBySlot[slot.slotId] ?: 0
                appendLine("  ${slot.slotId.padEnd(4)}: $pName (hợp $fit%)")
            }
            appendLine("  Đội trưởng: ${playerBySlot[sp.captainSlotIds.firstOrNull()] ?: "-"}")
            appendLine("  Phạt đền: ${playerBySlot[sp.penaltyTakerSlotIds.firstOrNull()] ?: "-"}")
            appendLine("  Phạt góc: ${playerBySlot[sp.cornerTakerSlotIds.firstOrNull()] ?: "-"}")
            appendLine("  Đá phạt gần: ${playerBySlot[sp.closeFreeKickTakerSlotIds.firstOrNull()] ?: "-"}")
            appendLine("  Dự bị (${plan.bench.size}): ${plan.bench.take(5).joinToString(", ") { it.displayName }}")
        }
    }

    fun createRustInputJson(
        homeName: String,
        awayName: String,
        seed: Long,
        simulatedActionMinutes: Int = 20,
        knockout: Boolean = false,
        homeTactics: LabTacticalPresets.TacticalConfig = LabTacticalPresets.TacticalConfig(),
        awayTactics: LabTacticalPresets.TacticalConfig = LabTacticalPresets.TacticalConfig(),
        aiHomeManager: Boolean = false,
        aiAwayManager: Boolean = true
    ): String {
        require(homeName.isNotBlank() && awayName.isNotBlank() && homeName != awayName) { "Invalid teams" }
        val homeId = findClubId(homeName)
        val awayId = findClubId(awayName)
        val root = JSONObject().apply {
            put("matchId", "LAB-ENG-V2-$homeId-$awayId")
            put("seed", seed)
            put("engineVersion", ENGINE_VERSION)
            put("neutralGround", false)
            put("competitionContext", findClub(homeName).optString("league_id", "ENG_1"))
            put("simulatedActionMinutes", simulatedActionMinutes)
            put("rulesMode", if (knockout) "KNOCKOUT" else "STANDARD")
            put("aiHomeManager", aiHomeManager)
            put("aiAwayManager", aiAwayManager)
            put("homeTeam", serializeTeam(homeId, homeName, homeTactics))
            put("awayTeam", serializeTeam(awayId, awayName, awayTactics))
        }
        return root.toString()
    }

    private fun findClub(teamName: String): JSONObject {
        val teams = source.getJSONArray("clubs")
        for (i in 0 until teams.length()) {
            val t = teams.getJSONObject(i)
            if (teamName == t.optString("name")) return t
        }
        throw IllegalArgumentException("Unknown English-pyramid club: $teamName")
    }

    private fun findClubId(teamName: String): String = findClub(teamName).getString("id")

    private fun serializeTeam(
        teamId: String,
        teamName: String,
        tactics: LabTacticalPresets.TacticalConfig
    ): JSONObject {
        val squad = loadSquadSnapshots(teamName)
        val plan = LabTacticalPresets.planOptimalLineup(squad, tactics.formation, benchSize = 9)
        val lineupArr = JSONArray()
        for (p in plan.starters) lineupArr.put(serializePlayer(p))
        val benchArr = JSONArray()
        for (p in plan.bench) benchArr.put(serializePlayer(p))
        val assignArr = JSONArray()
        for (a in plan.assignments) {
            assignArr.put(JSONObject().put("playerId", a.playerId).put("slotId", a.slotId))
        }
        val slotArr = JSONArray()
        val siArr = JSONArray()
        for (s in tactics.formation.slots) {
            slotArr.put(
                JSONObject()
                    .put("slotId", s.slotId)
                    .put("outX", s.outX).put("outY", s.outY)
                    .put("inX", s.inX).put("inY", s.inY)
            )
            val si = LabTacticalPresets.defaultSlotInstructions(s)
            siArr.put(
                JSONObject()
                    .put("slotId", si.slotId)
                    .put("forwardMovement", si.forwardMovement)
                    .put("passingRisk", si.passingRisk)
                    .put("crossingTendency", si.crossingTendency)
                    .put("dribblingTendency", si.dribblingTendency)
                    .put("defensiveAggression", si.defensiveAggression)
                    .put("supportDistance", si.supportDistance)
                    .put("roaming", si.roaming)
                    .put("shootingTendency", si.shootingTendency)
            )
        }
        val st = tactics.style
        val tiObj = JSONObject()
            .put("pressingIntensity", st.pressing)
            .put("tempo", st.tempo)
            .put("lineHeight", st.lineHeight)
            .put("width", st.width)
            .put("timeWasting", st.timeWasting)
            .put("offsideTrap", st.offsideTrap)
            .put("aggression", st.aggression)
            .put("transitionRisk", st.transitionRisk)
            .put("buildUpRisk", st.buildUpRisk)
            .put("concentration", st.concentration)
        val sp = plan.setPiecePlan
        val spObj = JSONObject()
            .put("penaltyTakerSlotIds", JSONArray(sp.penaltyTakerSlotIds))
            .put("cornerTakerSlotIds", JSONArray(sp.cornerTakerSlotIds))
            .put("closeFreeKickTakerSlotIds", JSONArray(sp.closeFreeKickTakerSlotIds))
            .put("farFreeKickTakerSlotIds", JSONArray(sp.farFreeKickTakerSlotIds))
            .put("captainSlotIds", JSONArray(sp.captainSlotIds))
            .put("cornerBoxSlotIds", JSONArray(sp.cornerBoxSlotIds))
            .put("closeFreeKickBoxSlotIds", JSONArray(sp.closeFreeKickBoxSlotIds))
            .put("farFreeKickBoxSlotIds", JSONArray(sp.farFreeKickBoxSlotIds))
        val collObj = JSONObject()
            .put("cohesion", 50)
            .put("systemFamiliarity", 50)
            .put("confidence", 50)
        return JSONObject()
            .put("teamId", teamId)
            .put("displayName", teamName)
            .put("shapeId", tactics.formation.id)
            .put("designatedGoalkeeperId", plan.designatedGoalkeeperId)
            .put("lineup", lineupArr)
            .put("bench", benchArr)
            .put("assignments", assignArr)
            .put("tacticalSlots", slotArr)
            .put("teamInstructions", tiObj)
            .put("slotInstructions", siArr)
            .put("setPiecePlan", spObj)
            .put("collectiveState", collObj)
    }

    private fun serializePlayer(p: LabPlayerSnapshot): JSONObject {
        val attrs = JSONArray()
        for (v in p.attributes.toIntArray()) attrs.put(v)
        return JSONObject()
            .put("playerId", p.playerId)
            .put("displayName", p.displayName)
            .put("attributes", attrs)
            .put("condition", p.condition)
            .put("morale", p.morale)
            .put("form", p.form)
            .put("age", p.age)
            .put("injured", p.isInjured)
            .put("eligible", p.isEligible)
            .put("primaryPosition", p.primaryPosition)
            .put("secondaryPosition", p.secondaryPosition)
    }

    /**
     * Development bridge: the 54-item stage-2 schema is the source of truth in
     * the database. The existing Rust JNI match engine still accepts the 20 V3
     * fields. Only project for exhibition matches; NOT full native 54-attr parity.
     */
    private fun project54ForV3(row: JSONObject): LabPlayerAttributes {
        val attr = row.getJSONObject("simulation").getJSONObject("attributes")
        fun v(group: String, name: String): Int =
            attr.getJSONObject(group).optInt(name, 10).coerceIn(1, 20)
        fun scale(n: Int): Int = (1 + ((n.coerceIn(1, 20) - 1) * 98.0 / 19.0)).toInt().coerceIn(1, 99)
        fun avg(vararg vals: Int) = vals.sum() / vals.size
        return LabPlayerAttributes(
            goalkeeping = scale(avg(v("goalkeeping", "reflexes"), v("goalkeeping", "handling"), v("goalkeeping", "one_on_ones"))),
            tackling = scale(v("technical", "tackling")),
            passing = scale(v("technical", "passing")),
            crossing = scale(v("technical", "crossing")),
            technique = scale(v("technical", "technique")),
            dribbling = scale(v("technical", "dribbling")),
            shooting = scale(avg(v("technical", "finishing"), v("technical", "long_shots"))),
            heading = scale(v("technical", "heading")),
            pace = scale(v("physical", "pace")),
            agility = scale(v("physical", "agility")),
            strength = scale(v("physical", "strength")),
            stamina = scale(v("physical", "stamina")),
            vision = scale(v("mental", "vision")),
            decisions = scale(v("mental", "decisions")),
            composure = scale(v("mental", "composure")),
            offBallMovement = scale(v("mental", "off_the_ball")),
            defensivePositioning = scale(v("mental", "positioning")),
            workRate = scale(v("mental", "work_rate")),
            charisma = scale(v("mental", "leadership")),
            temperament = scale(v("hidden", "temperament"))
        )
    }

    private fun loadSquadSnapshots(teamName: String): List<LabPlayerSnapshot> {
        val club = findClub(teamName)
        val league = club.getString("league_id")
        val idClub = club.getString("id")
        val path = "england_v2/division_${league}.json"
        val division = JSONObject(
            appContext.assets.open(path).bufferedReader(Charsets.UTF_8).use { it.readText() }
        )
        val roster = ArrayList<JSONObject>()
        val players = division.getJSONArray("players")
        for (i in 0 until players.length()) {
            val row = players.getJSONObject(i)
            if (row.optString("club_id") == idClub) roster.add(row)
        }
        check(roster.size >= 18) { "Fewer than 18 registered players: $teamName ($league)" }
        roster.sortWith(compareByDescending<JSONObject> { it.optInt("skill") }.thenBy { it.optString("id") })
        return roster.map { row ->
            val id = row.getString("id")
            val playerName = row.optString("name")
            playerNames[id] = playerName
            LabPlayerSnapshot(
                playerId = id,
                displayName = playerName,
                attributes = project54ForV3(row),
                condition = 100,
                morale = 65,
                form = 55,
                age = row.optInt("age", 24).coerceIn(1, 80),
                isInjured = false,
                isEligible = true,
                primaryPosition = row.optString("position"),
                secondaryPosition = row.optString("second_position")
            )
        }
    }
}
