package pl.mkrstudio.truefootball3.match_playback

import pl.mkrstudio.truefootball3.lab.*

import org.json.JSONObject

data class RosterEntry(
        val id: String,
        val name: String,
        val position: String
    ) {
        val label: String get() = "$name ($position)"
    }

/** Main-thread projection of the authoritative roster. No native calls, command
 * validation, or speculative substitutions. UI reads copies through List.
 */
class MatchRosterProjection {
    private val homeRosterIndex = HashMap<String, RosterEntry>()
    private val onPitch = ArrayList<RosterEntry>()
    private val bench = ArrayList<RosterEntry>()
    val homeOnPitch: List<RosterEntry> get() = onPitch.toList()
    val homeBench: List<RosterEntry> get() = bench.toList()
    var homeSubsUsed = 0
        private set
    var homeWindowsUsed = 0
        private set
    var intervalSubstitutionAllowed = false
        private set

    fun syncV4HomeRoster(authoritative: JSONObject) {
        fun getPlayers(field: String): List<RosterEntry> {
            val ids = authoritative.optJSONArray(field) ?: return emptyList()
            return (0 until ids.length()).mapNotNull { idx ->
                val id = ids.optString(idx)
                homeRosterIndex[id]
            }
        }
        onPitch.clear()
        onPitch.addAll(getPlayers("onPitchPlayerIds"))
        bench.clear()
        bench.addAll(getPlayers("availableBenchPlayerIds"))
        homeSubsUsed = authoritative.optInt("substitutionsUsed", homeSubsUsed)
        homeWindowsUsed = authoritative.optInt("substitutionWindowsUsed", homeWindowsUsed)
        intervalSubstitutionAllowed = authoritative.optBoolean("intervalSubstitutionAllowed", false)
    }

    fun populateHomeRoster(inputJson: String) {
        val homeObj = JSONObject(inputJson).optJSONObject("homeTeam") ?: return
        val lineupArr = homeObj.optJSONArray("lineup")
        val benchArr = homeObj.optJSONArray("bench")
        val starters = ArrayList<RosterEntry>()
        val subs = ArrayList<RosterEntry>()
        if (lineupArr != null) {
            for (i in 0 until lineupArr.length()) {
                val p = lineupArr.getJSONObject(i)
                starters.add(
                    RosterEntry(
                        p.optString("playerId"),
                        p.optString("displayName"),
                        p.optString("primaryPosition", "CM")
                    )
                )
            }
        }
        if (benchArr != null) {
            for (i in 0 until benchArr.length()) {
                val p = benchArr.getJSONObject(i)
                subs.add(
                    RosterEntry(
                        p.optString("playerId"),
                        p.optString("displayName"),
                        p.optString("primaryPosition", "CM")
                    )
                )
            }
        }
        run {
            homeRosterIndex.clear()
            for (entry in starters + subs) homeRosterIndex[entry.id] = entry
            onPitch.clear()
            onPitch.addAll(starters)
            bench.clear()
            bench.addAll(subs)
            homeSubsUsed = 0
            homeWindowsUsed = 0
            intervalSubstitutionAllowed = false
        }
    }
}
