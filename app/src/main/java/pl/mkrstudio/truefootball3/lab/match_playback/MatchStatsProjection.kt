package pl.mkrstudio.truefootball3.match_playback

import pl.mkrstudio.truefootball3.lab.*

import org.json.JSONObject

/** Pure final-result formatting; no Activity or JNI access. */
object MatchStatsProjection {
    fun render(root: JSONObject, home: String, away: String, viewingMode: LabViewingMode, seed: Long, name: ((String?) -> String)?): String {
        val summary = root.optJSONObject("summary")
        val hs = summary?.optJSONObject("home")
        val asObj = summary?.optJSONObject("away")
        return buildString {
            appendLine("Động cơ: Rust Engine 4.1 DES (${viewingMode.labelVi}) | Seed: $seed")
            if (hs != null && asObj != null) {
                appendLine("Tỷ số chung cuộc: $home ${hs.optInt("goals")} – ${asObj.optInt("goals")} $away")
                if (summary.optBoolean("wentToShootout", false)) {
                    appendLine("Luân lưu: ${summary.optInt("shootoutHomeGoals")} – ${summary.optInt("shootoutAwayGoals")}")
                }
                appendLine("Kiểm soát bóng: ${"%.1f".format(hs.optDouble("possessionPct"))}% – ${"%.1f".format(asObj.optDouble("possessionPct"))}%")
                appendLine("Sút (trúng đích): ${hs.optInt("shots")} (${hs.optInt("shotsOnTarget")}) – ${asObj.optInt("shots")} (${asObj.optInt("shotsOnTarget")})")
                appendLine("Sút chệch/bị chặn: ${hs.optInt("shotsOffTarget")}/${hs.optInt("shotsBlocked")} – ${asObj.optInt("shotsOffTarget")}/${asObj.optInt("shotsBlocked")}")
                appendLine("Cứu thua: ${hs.optInt("saves")} – ${asObj.optInt("saves")} | Phản lưới nhà: ${hs.optInt("ownGoalsConceded")} – ${asObj.optInt("ownGoalsConceded")}")
                appendLine("xG: ${"%.2f".format(hs.optDouble("xg"))} – ${"%.2f".format(asObj.optDouble("xg"))}")
                appendLine("Chuyền thành công: ${hs.optInt("passesCompleted")}/${hs.optInt("passesAttempted")} – ${asObj.optInt("passesCompleted")}/${asObj.optInt("passesAttempted")}")
                appendLine("Phạt góc: ${hs.optInt("corners")} – ${asObj.optInt("corners")} | Việt vị: ${hs.optInt("offsides")} – ${asObj.optInt("offsides")}")
                appendLine("Ném biên: ${hs.optInt("throwIns")} – ${asObj.optInt("throwIns")} | Phạm lỗi: ${hs.optInt("fouls")} – ${asObj.optInt("fouls")}")
                appendLine("Thẻ vàng/đỏ: ${hs.optInt("yellowCards")}/${hs.optInt("redCards")} – ${asObj.optInt("yellowCards")}/${asObj.optInt("redCards")}")
            }
            val players = summary?.optJSONArray("players")
            if (players != null && players.length() > 0) {
                val sorted = (0 until players.length())
                    .map { players.getJSONObject(it) }
                    .sortedByDescending { it.optDouble("rating", 6.0) }
                appendLine("\nChấm điểm nổi bật:")
                for (p in sorted.take(6)) {
                    val pName = name?.invoke(p.optString("playerId")) ?: p.optString("playerId")
                    val teamTag = if (p.optString("side") == "HOME") "Nhà" else "Khách"
                    val rStr = "%.1f".format(p.optDouble("rating", 6.0))
                    val distKm = "%.1f".format(p.optDouble("distanceM", 0.0) / 1000.0)
                    val ownGoalTag = if (p.optInt("ownGoals") > 0) ", ${p.optInt("ownGoals")} phản lưới" else ""
                    appendLine("  • $pName ($teamTag): $rStr (${p.optInt("goals")}B/${p.optInt("assists")}KT$ownGoalTag • ${distKm}km)")
                }
            }
            appendLine("\nTổng sự kiện: ${root.optInt("eventCount")} | FP: ${root.optString("eventTimelineFingerprint")}")
        }
    }
}
