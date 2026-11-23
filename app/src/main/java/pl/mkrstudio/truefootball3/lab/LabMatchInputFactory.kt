package pl.mkrstudio.truefootball3.lab

import android.content.Context
import org.json.JSONArray
import org.json.JSONObject
import java.util.Locale
import java.util.Random

class LabMatchInputFactory(context: Context, assetPath: String = "england_v2/clubs.json") {
    private val appContext = context.applicationContext
    private val source: JSONObject = JSONObject(
        appContext.assets.open(assetPath).bufferedReader(Charsets.UTF_8).use { it.readText() }
    )
    private val playerNames = HashMap<String, String>()

    companion object {
        const val ENGINE_VERSION = "v3.17.0-realism-1"
        const val ENGINE_VERSION_V4 = "v4.0.0"
        const val ENGINE_PROTOCOL_V4 = 4
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

    // --- V3 Reference JSON Factory (preserved 100%) ---

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

    // --- V4.0 Native 54-Attribute 90-Minute JSON Factory ---

    fun createRustV4InputJson(
        homeName: String,
        awayName: String,
        seed: Long,
        knockout: Boolean = false,
        simulationMode: LabSimulationMode = LabSimulationMode.FULL_EVENT,
        matchInstanceId: String? = null,
        homeTactics: LabTacticalPresets.TacticalConfig = LabTacticalPresets.TacticalConfig(),
        awayTactics: LabTacticalPresets.TacticalConfig = LabTacticalPresets.TacticalConfig(),
        aiHomeManager: Boolean = false,
        aiAwayManager: Boolean = true,
        environment: LabMatchEnvironment = LabMatchEnvironment(),
        referee: LabRefereeProfile = LabRefereeProfile()
    ): String {
        require(homeName.isNotBlank() && awayName.isNotBlank() && homeName != awayName) { "Invalid teams" }
        val homeId = findClubId(homeName)
        val awayId = findClubId(awayName)
        val root = JSONObject().apply {
            put("protocolVersion", ENGINE_PROTOCOL_V4)
            put("matchId", matchInstanceId?.also { supplied ->
                require(supplied.matches(Regex("[A-Za-z0-9._-]{8,160}"))) {
                    "Invalid immutable fixture matchInstanceId"
                }
            } ?: "LAB-V4-$homeId-$awayId-${java.util.UUID.randomUUID()}")
            put("seed", seed)
            put("engineVersion", ENGINE_VERSION_V4)
            put("neutralGround", false)
            put("competitionContext", findClub(homeName).optString("league_id", "ENG_1"))
            put("rulesMode", if (knockout) "KNOCKOUT" else "STANDARD")
            put("simulationMode", simulationMode.name)
            put("aiHomeManager", aiHomeManager)
            put("aiAwayManager", aiAwayManager)
            put(
                "matchEnvironment",
                JSONObject()
                    .put("weather", environment.weather.name)
                    .put("pitchQuality", environment.pitchQuality.name)
                    .put("windStrength", environment.windStrength.coerceIn(0, 100))
                    .put("homeCrowdIntensity", environment.homeCrowdIntensity.coerceIn(0, 100))
                    .put("matchImportance", environment.matchImportance.coerceIn(0, 100))
            )
            put(
                "refereeProfile",
                JSONObject()
                    .put("strictness", referee.strictness.coerceIn(0, 100))
                    .put("cardThreshold", referee.cardThreshold.coerceIn(0, 100))
                    .put("advantageTendency", referee.advantageTendency.coerceIn(0, 100))
            )
            put("homeTeam", serializeTeamV4(homeId, homeName, homeTactics))
            put("awayTeam", serializeTeamV4(awayId, awayName, awayTactics))
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

    private fun serializeTeamV4(
        teamId: String,
        teamName: String,
        tactics: LabTacticalPresets.TacticalConfig
    ): JSONObject {
        val rows = loadRawClubRows(teamName)
        val v4ById = HashMap<String, LabPlayerSnapshotV4>(rows.size)
        val v3ForPlanning = ArrayList<LabPlayerSnapshot>(rows.size)
        for (row in rows) {
            val id = row.getString("id")
            val playerName = row.optString("name")
            playerNames[id] = playerName
            v4ById[id] = parsePlayerV4(row)
            v3ForPlanning.add(
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
            )
        }
        val plan = LabTacticalPresets.planOptimalLineup(v3ForPlanning, tactics.formation, benchSize = 9)
        val lineupArr = JSONArray()
        for (p in plan.starters) lineupArr.put(serializePlayerV4(v4ById.getValue(p.playerId)))
        val benchArr = JSONArray()
        for (p in plan.bench) benchArr.put(serializePlayerV4(v4ById.getValue(p.playerId)))

        return buildCommonTeamPayload(teamId, teamName, tactics, plan, lineupArr, benchArr)
    }

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

        return buildCommonTeamPayload(teamId, teamName, tactics, plan, lineupArr, benchArr)
    }

    private fun buildCommonTeamPayload(
        teamId: String,
        teamName: String,
        tactics: LabTacticalPresets.TacticalConfig,
        plan: LabTacticalPresets.PlannedLineup,
        lineupArr: JSONArray,
        benchArr: JSONArray
    ): JSONObject {
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

    private fun serializePlayerV4(p: LabPlayerSnapshotV4): JSONObject {
        val a = p.attributes
        val attrsObj = JSONObject()
            .put(
                "technical",
                JSONObject()
                    .put("finishing", a.finishing)
                    .put("passing", a.passing)
                    .put("dribbling", a.dribbling)
                    .put("technique", a.technique)
                    .put("first_touch", a.firstTouch)
                    .put("crossing", a.crossing)
                    .put("heading", a.heading)
                    .put("long_shots", a.longShots)
                    .put("tackling", a.tackling)
                    .put("marking", a.marking)
                    .put("free_kicks", a.freeKicks)
                    .put("penalties", a.penalties)
                    .put("corners", a.corners)
            )
            .put(
                "mental",
                JSONObject()
                    .put("vision", a.vision)
                    .put("decisions", a.decisions)
                    .put("composure", a.composure)
                    .put("concentration", a.concentration)
                    .put("off_the_ball", a.offTheBall)
                    .put("positioning", a.positioning)
                    .put("anticipation", a.anticipation)
                    .put("teamwork", a.teamwork)
                    .put("determination", a.determination)
                    .put("work_rate", a.workRate)
                    .put("leadership", a.leadership)
                    .put("flair", a.flair)
                    .put("aggression", a.aggression)
                    .put("bravery", a.bravery)
            )
            .put(
                "physical",
                JSONObject()
                    .put("pace", a.pace)
                    .put("acceleration", a.acceleration)
                    .put("strength", a.strength)
                    .put("stamina", a.stamina)
                    .put("balance", a.balance)
                    .put("agility", a.agility)
                    .put("jumping_reach", a.jumpingReach)
                    .put("natural_fitness", a.naturalFitness)
            )
            .put(
                "goalkeeping",
                JSONObject()
                    .put("reflexes", a.reflexes)
                    .put("handling", a.handling)
                    .put("one_on_ones", a.oneOnOnes)
                    .put("command_of_area", a.commandOfArea)
                    .put("aerial_reach", a.aerialReach)
                    .put("kicking", a.kicking)
                    .put("throwing", a.throwing)
                    .put("rushing_out", a.rushingOut)
                    .put("punching", a.punching)
            )
            .put(
                "hidden",
                JSONObject()
                    .put("professionalism", a.professionalism)
                    .put("ambition", a.ambition)
                    .put("consistency", a.consistency)
                    .put("big_matches", a.bigMatches)
                    .put("adaptability", a.adaptability)
                    .put("loyalty", a.loyalty)
                    .put("pressure", a.pressure)
                    .put("injury_proneness", a.injuryProneness)
                    .put("temperament", a.temperament)
                    .put("sportsmanship", a.sportsmanship)
            )

        return JSONObject()
            .put("playerId", p.playerId)
            .put("displayName", p.displayName)
            .put("attributes", attrsObj)
            .put("condition", p.condition)
            .put("morale", p.morale)
            .put("form", p.form)
            .put("age", p.age)
            .put("injured", p.isInjured)
            .put("eligible", p.isEligible)
            .put("primaryPosition", p.primaryPosition)
            .put("secondaryPosition", p.secondaryPosition)
            .put("preferredFoot", p.preferredFoot.name)
            .put("weakFoot", p.weakFoot)
            .put("heightCm", p.heightCm)
    }

    private fun parsePlayerV4(row: JSONObject): LabPlayerSnapshotV4 {
        val id = row.getString("id")
        val playerName = row.optString("name")
        val primaryPos = row.optString("position")
        val secondaryPos = row.optString("second_position")
        val attr = row.getJSONObject("simulation").getJSONObject("attributes")
        fun v(group: String, key: String): Int =
            attr.getJSONObject(group).optInt(key, 10).coerceIn(1, 20)

        val attrs54 = LabPlayerAttributes54(
            finishing = v("technical", "finishing"),
            passing = v("technical", "passing"),
            dribbling = v("technical", "dribbling"),
            technique = v("technical", "technique"),
            firstTouch = v("technical", "first_touch"),
            crossing = v("technical", "crossing"),
            heading = v("technical", "heading"),
            longShots = v("technical", "long_shots"),
            tackling = v("technical", "tackling"),
            marking = v("technical", "marking"),
            freeKicks = v("technical", "free_kicks"),
            penalties = v("technical", "penalties"),
            corners = v("technical", "corners"),
            vision = v("mental", "vision"),
            decisions = v("mental", "decisions"),
            composure = v("mental", "composure"),
            concentration = v("mental", "concentration"),
            offTheBall = v("mental", "off_the_ball"),
            positioning = v("mental", "positioning"),
            anticipation = v("mental", "anticipation"),
            teamwork = v("mental", "teamwork"),
            determination = v("mental", "determination"),
            workRate = v("mental", "work_rate"),
            leadership = v("mental", "leadership"),
            flair = v("mental", "flair"),
            aggression = v("mental", "aggression"),
            bravery = v("mental", "bravery"),
            pace = v("physical", "pace"),
            acceleration = v("physical", "acceleration"),
            strength = v("physical", "strength"),
            stamina = v("physical", "stamina"),
            balance = v("physical", "balance"),
            agility = v("physical", "agility"),
            jumpingReach = v("physical", "jumping_reach"),
            naturalFitness = v("physical", "natural_fitness"),
            reflexes = v("goalkeeping", "reflexes"),
            handling = v("goalkeeping", "handling"),
            oneOnOnes = v("goalkeeping", "one_on_ones"),
            commandOfArea = v("goalkeeping", "command_of_area"),
            aerialReach = v("goalkeeping", "aerial_reach"),
            kicking = v("goalkeeping", "kicking"),
            throwing = v("goalkeeping", "throwing"),
            rushingOut = v("goalkeeping", "rushing_out"),
            punching = v("goalkeeping", "punching"),
            professionalism = v("hidden", "professionalism"),
            ambition = v("hidden", "ambition"),
            consistency = v("hidden", "consistency"),
            bigMatches = v("hidden", "big_matches"),
            adaptability = v("hidden", "adaptability"),
            loyalty = v("hidden", "loyalty"),
            pressure = v("hidden", "pressure"),
            injuryProneness = v("hidden", "injury_proneness"),
            temperament = v("hidden", "temperament"),
            sportsmanship = v("hidden", "sportsmanship")
        )

        val (foot, weakFoot, heightCm) = derivePhysicalMeta(
            row = row,
            playerId = id,
            position = primaryPos,
            jumpingReach = attrs54.jumpingReach
        )

        return LabPlayerSnapshotV4(
            playerId = id,
            displayName = playerName,
            attributes = attrs54,
            condition = row.optInt("condition", 100).coerceIn(10, 100),
            morale = row.optInt("morale", 65).coerceIn(0, 100),
            form = row.optInt("form", 55).coerceIn(0, 100),
            age = row.optInt("age", 24).coerceIn(15, 50),
            isInjured = row.optBoolean("injured", false),
            isEligible = row.optBoolean("eligible", true),
            primaryPosition = primaryPos,
            secondaryPosition = secondaryPos,
            preferredFoot = foot,
            weakFoot = weakFoot,
            heightCm = heightCm
        )
    }

    /**
     * Deterministically derives `preferredFoot`, `weakFoot`, and `heightCm` from
     * `hash(playerId) + position + jumping_reach` when not explicitly present in the database JSON.
     */
    private fun derivePhysicalMeta(
        row: JSONObject,
        playerId: String,
        position: String,
        jumpingReach: Int
    ): Triple<LabPreferredFoot, Int, Int> {
        var seed = -3750763034362895579L
        for (ch in "V4META|$playerId") {
            seed = seed xor ch.code.toLong()
            seed *= 1099511628211L
        }
        val rng = Random(seed)
        val pos = position.trim().uppercase(Locale.ROOT)

        val explicitFoot = row.optString("preferred_foot", "").trim().uppercase(Locale.ROOT)
        val foot = when (explicitFoot) {
            "LEFT" -> LabPreferredFoot.LEFT
            "RIGHT" -> LabPreferredFoot.RIGHT
            "EITHER", "BOTH" -> LabPreferredFoot.EITHER
            else -> {
                val roll = rng.nextInt(100)
                when {
                    pos in setOf("LB", "LWB", "LW", "LM") -> when {
                        roll < 65 -> LabPreferredFoot.LEFT
                        roll < 95 -> LabPreferredFoot.RIGHT
                        else -> LabPreferredFoot.EITHER
                    }
                    pos in setOf("RB", "RWB", "RW", "RM") -> when {
                        roll < 78 -> LabPreferredFoot.RIGHT
                        roll < 95 -> LabPreferredFoot.LEFT
                        else -> LabPreferredFoot.EITHER
                    }
                    else -> when {
                        roll < 74 -> LabPreferredFoot.RIGHT
                        roll < 95 -> LabPreferredFoot.LEFT
                        else -> LabPreferredFoot.EITHER
                    }
                }
            }
        }

        val weakFoot = if (row.has("weak_foot")) {
            row.optInt("weak_foot", 3).coerceIn(1, 5)
        } else if (foot == LabPreferredFoot.EITHER) {
            5
        } else {
            (2 + rng.nextInt(3)).coerceIn(1, 5)
        }

        val heightCm = if (row.has("height_cm")) {
            row.optInt("height_cm", 180).coerceIn(155, 215)
        } else {
            val baseHeight = when {
                pos == "GK" -> 189
                pos in setOf("CB", "SW", "B") -> 186
                pos in setOf("CF", "ST", "FW", "F") -> 181
                pos in setOf("DM", "CDM", "CM", "M") -> 179
                else -> 176
            }
            val jumpDelta = ((jumpingReach - 10) * 0.65).toInt()
            val jitter = rng.nextInt(9) - 4
            (baseHeight + jumpDelta + jitter).coerceIn(165, 204)
        }

        return Triple(foot, weakFoot, heightCm)
    }

    /**
     * Development bridge: the 54-item stage-2 schema is the source of truth in
     * the database. The existing Rust JNI match engine still accepts the 20 V3
     * fields. Only project for V3 parity/exhibition matches.
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

    private fun loadRawClubRows(teamName: String): List<JSONObject> {
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
        return roster
    }

    private fun loadSquadSnapshots(teamName: String): List<LabPlayerSnapshot> {
        return loadRawClubRows(teamName).map { row ->
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
