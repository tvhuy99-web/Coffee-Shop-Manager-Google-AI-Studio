package pl.mkrstudio.truefootball3.match_commentary

import pl.mkrstudio.truefootball3.lab.*

import org.json.JSONObject
import kotlin.math.roundToInt

/** Owns the event-to-commentary presentation projection, not the Activity.
 * The projection preserves legacy event filtering, score context, minute labels
 * and fallback text. It does not call JNI or mutate Rust state.
 */
class MatchEventCueAdapter {
    var liveGameSecond = 0
        private set
    private var lastV4PlayingPeriod = 1
    private var runningHomeGoals = 0
    private var runningAwayGoals = 0

    fun initializeScore(homeGoals: Int, awayGoals: Int) {
        runningHomeGoals = homeGoals
        runningAwayGoals = awayGoals
    }

    fun convertV4StepEvents(
        stepObj: JSONObject,
        rawEvents: List<JSONObject>,
        filteredEvents: List<JSONObject>,
        viewingMode: LabViewingMode,
        name: (String?) -> String,
        render: (JSONObject, LabEventType, LabTeamSide?, String, String, Int, Int, Int) -> String?
    ): List<NarrationScheduler.SimulationEvent> {
        val filteredSeqs = HashSet<Int>(filteredEvents.size)
        for (fe in filteredEvents) {
            filteredSeqs.add(fe.optInt("sequence", -1))
        }
        val out = ArrayList<NarrationScheduler.SimulationEvent>(filteredEvents.size)
        for (ev in rawEvents) {
            val kindName = ev.optString("kind", "")
            val kind = try {
                LabEventType.valueOf(kindName)
            } catch (_: IllegalArgumentException) {
                continue
            }
            val side = when (ev.optString("side", "")) {
                "HOME" -> LabTeamSide.HOME
                "AWAY" -> LabTeamSide.AWAY
                else -> null
            }
            if (kind == LabEventType.GOAL || kind == LabEventType.OWN_GOAL) {
                // In Rust V4, EventTypeV4::OwnGoal carries the beneficiary side (same as Goal).
                if (side == LabTeamSide.HOME) runningHomeGoals++
                else if (side == LabTeamSide.AWAY) runningAwayGoals++
            }
            val seq = ev.optInt("sequence", -1)
            if (viewingMode != LabViewingMode.FULL_MATCH && seq !in filteredSeqs) {
                continue
            }
            val period = ev.optInt("period", stepObj.optInt("period", 1))
            if (period in 1..4) lastV4PlayingPeriod = period
            val evSec = ev.optInt("gameSecond", liveGameSecond)
            val simTimeMs = ev.optLong("simTimeMs", evSec * 1000L)
            val periodElapsedSec = ev.optInt("periodElapsedSecond", -1).takeIf { it >= 0 }
            val periodElapsedMs = ev.optLong("periodElapsedMs", (periodElapsedSec ?: 0) * 1000L)
            val minuteInPeriod = if (periodElapsedSec != null) {
                ((periodElapsedSec + 59) / 60).coerceAtLeast(0)
            } else ((evSec + 59) / 60).coerceAtLeast(1)
            val minute = when (period) {
                2 -> 45 + minuteInPeriod
                3 -> 90 + minuteInPeriod
                4 -> 105 + minuteInPeriod
                5, 6 -> if (lastV4PlayingPeriod >= 3) 120 else 90
                else -> minuteInPeriod
            }.coerceAtLeast(1)
            val displayMin = formatDisplayMinute(period, evSec, periodElapsedSec)
            val actorName = name(ev.optString("actor", "").takeIf { it != "null" })
            val targetName = name(ev.optString("target", "").takeIf { it != "null" })

            val normalized = JSONObject(ev.toString())
            val ballPos = ev.optJSONObject("ballPos")
            if (ballPos != null) {
                val bx = ((ballPos.optDouble("x", 52.5) / 105.0) * 100.0).roundToInt().coerceIn(0, 100)
                normalized.put("ballX", bx)
            }
            val text = render(
                normalized, kind, side, actorName, targetName, minute, runningHomeGoals, runningAwayGoals
            ) ?: fallbackV4Text(kind, actorName, targetName) ?: continue

            val metaObj = ev.optJSONObject("metadata")
            val isWoodwork = metaObj?.optString("outcome")?.equals("WOODWORK", ignoreCase = true) == true
            val isPenalty = kind == LabEventType.PENALTY_KICK || kind == LabEventType.PENALTY_AWARDED
            val shootoutScored = if (kind == LabEventType.PENALTY_SHOOTOUT_KICK) {
                metaObj?.optBoolean("scored", false)
            } else null

            LabLogStore.log(
                "EVENT",
                "${displayMin}' [V4:$kind] simTimeMs=$simTimeMs side=${side ?: "-"} actor=$actorName target=$targetName score=$runningHomeGoals-$runningAwayGoals | $text"
            )
            out.add(
                NarrationScheduler.SimulationEvent(
                    sequence = seq,
                    simTimeMs = simTimeMs,
                    period = period,
                    periodElapsedMs = periodElapsedMs,
                    minute = minute,
                    displayMinute = displayMin,
                    kind = kind,
                    side = side,
                    homeGoals = runningHomeGoals,
                    awayGoals = runningAwayGoals,
                    text = text,
                    highlightTier = ev.optString("highlightTier", "NONE"),
                    isWoodwork = isWoodwork,
                    isPenalty = isPenalty,
                    isShootoutScored = shootoutScored
                )
            )
        }
        runningHomeGoals = stepObj.optInt("homeGoals", runningHomeGoals)
        runningAwayGoals = stepObj.optInt("awayGoals", runningAwayGoals)
        liveGameSecond = stepObj.optInt("gameSecond", liveGameSecond)
        return out
    }

    fun formatDisplayMinute(
        period: Int,
        gameSecond: Int,
        periodElapsedSecond: Int? = null
    ): String {
        if (periodElapsedSecond != null) {
            val elapsed = periodElapsedSecond.coerceAtLeast(0)
            val base = when (period) {
                1 -> 0
                2 -> 45
                3 -> 90
                4 -> 105
                5 -> 120
                6 -> if (lastV4PlayingPeriod >= 3) 120 else 90
                else -> 0
            }
            val regulationLength = if (period == 3 || period == 4) 15 else 45
            val minuteInPeriod = if (period >= 5) 0
            else ((elapsed + 59) / 60).coerceAtLeast(if (elapsed == 0) 0 else 1)
            val minute = base + minuteInPeriod
            return if (period in 1..4 && minuteInPeriod > regulationLength) {
                "${base + regulationLength}+${minuteInPeriod - regulationLength}"
            } else {
                minute.toString()
            }
        }
        val minute = ((gameSecond + 59) / 60).coerceAtLeast(1)
        return when {
            period == 1 && minute > 45 -> "45+${minute - 45}"
            period == 2 && minute > 90 -> "90+${minute - 90}"
            period == 3 && minute > 105 -> "105+${minute - 105}"
            period == 4 && minute > 120 -> "120+${minute - 120}"
            else -> minute.toString()
        }
    }

    private fun fallbackV4Text(kind: LabEventType, actor: String, target: String): String? {
        return when (kind) {
            LabEventType.GOAL -> "VÀO! $actor ghi bàn${if (target.isNotEmpty()) " sau đường kiến tạo của $target" else ""}!"
            LabEventType.OWN_GOAL -> "PHẢN LƯỚI NHÀ! $actor vô tình đưa bóng về lưới nhà."
            LabEventType.SHOT_ON_TARGET -> "$actor tung cú dứt điểm trúng đích!"
            LabEventType.SHOT_OFF_TARGET -> "$actor dứt điểm chệch khung thành."
            LabEventType.SAVE -> "Thủ môn $actor cản phá xuất sắc!"
            LabEventType.OFFSIDE -> "Trọng tài phất cờ báo việt vị với $actor."
            LabEventType.THROW_IN -> "Bóng đi hết đường biên dọc, $actor thực hiện quả ném biên."
            LabEventType.CORNER -> "$actor chuẩn bị đá phạt góc."
            LabEventType.FREE_KICK -> "$actor thực hiện quả đá phạt."
            LabEventType.PENALTY_AWARDED -> "PHẠT ĐỀN! Trọng tài chỉ tay vào chấm 11m."
            LabEventType.PENALTY_KICK -> "$actor bước lên thực hiện quả phạt đền."
            LabEventType.FOUL -> "$actor phạm lỗi${if (target.isNotEmpty()) " với $target" else ""}."
            LabEventType.ADVANTAGE_PLAYED -> "Trọng tài cho hưởng phép lợi thế!"
            LabEventType.YELLOW_CARD -> "Thẻ vàng dành cho $actor."
            LabEventType.RED_CARD -> "THẺ ĐỎ! $actor bị truất quyền thi đấu!"
            LabEventType.INJURY -> "$actor gặp chấn thương và cần chăm sóc y tế."
            LabEventType.SUBSTITUTION -> "Thay người: $actor vào sân thay $target."
            LabEventType.HALF_TIME -> "Kết thúc hiệp một."
            LabEventType.EXTRA_TIME_STARTED -> "Bắt đầu hiệp phụ!"
            LabEventType.EXTRA_TIME_HALF_TIME -> "Kết thúc hiệp phụ thứ nhất."
            LabEventType.PENALTY_SHOOTOUT_STARTED -> "Hai đội bước vào loạt sút luân lưu cân não!"
            LabEventType.PENALTY_SHOOTOUT_ENDED -> "Loạt sút luân lưu đã khép lại!"
            LabEventType.FULL_TIME -> "Trọng tài thổi còi kết thúc trận đấu!"
            else -> null
        }
    }
}
