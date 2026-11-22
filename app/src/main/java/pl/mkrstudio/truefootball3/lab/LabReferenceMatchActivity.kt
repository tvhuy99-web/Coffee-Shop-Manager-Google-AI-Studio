package pl.mkrstudio.truefootball3.lab

import android.app.Activity
import android.app.AlertDialog
import android.graphics.Color
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ListView
import android.widget.TextView
import android.widget.Toast
import org.json.JSONObject
import java.util.ArrayList
import java.util.HashMap
import java.util.concurrent.Executors
import pl.mkrstudio.truefootball3.adapters.CommentaryRecordAdapter
import pl.mkrstudio.truefootball3.audio.MatchAudioManager
import pl.mkrstudio.truefootball3.audio.MatchTtsManager

class LabReferenceMatchActivity : Activity() {
    companion object {
        const val EXTRA_HOME = "home"
        const val EXTRA_AWAY = "away"
        const val EXTRA_HOME_FORMATION = "home_formation"
        const val EXTRA_HOME_STYLE = "home_style"
        const val EXTRA_AWAY_FORMATION = "away_formation"
        const val EXTRA_AWAY_STYLE = "away_style"
        const val EXTRA_SIM_MINUTES = "sim_minutes"
        const val EXTRA_KNOCKOUT = "knockout"
        const val EXTRA_SEED = "seed"
        private const val MAX_SUBSTITUTIONS = 5
    }

    private data class LabNarration(
        val minute: Int,
        val displayMinute: String,
        val homeGoals: Int,
        val awayGoals: Int,
        val text: String,
        val kind: LabEventType,
        val side: LabTeamSide?,
        val isWoodwork: Boolean = false,
        val isPenalty: Boolean = false,
        val isShootoutScored: Boolean? = null
    )

    private data class RosterEntry(
        val id: String,
        val name: String,
        val position: String
    ) {
        val label: String get() = "$name ($position)"
    }

    private val main = Handler(Looper.getMainLooper())
    private val executor = Executors.newSingleThreadExecutor()
    private val records = ArrayList<HashMap<String, Any>>()
    private val narration = ArrayList<LabNarration>()
    private val homeOnPitch = ArrayList<RosterEntry>()
    private val homeBench = ArrayList<RosterEntry>()
    private var homeSubsUsed = 0
    private var played = 0
    private var paused = false
    private var playbackMs = 320L
    private var finished = false
    private var liveGameSecond = 0
    private var statsSummaryText = "Chưa có thống kê trận đấu."

    private lateinit var header: TextView
    private lateinit var state: TextView
    private lateinit var list: ListView
    private lateinit var playButton: Button
    private lateinit var recordAdapter: CommentaryRecordAdapter

    private var tts: MatchTtsManager? = null
    private var audio: MatchAudioManager? = null
    private var audioDirector: LabAudioDirector? = null
    private var liveRustSession: LabRustLiveSession? = null
    private var rustCommentary: LabCommentaryEngine? = null
    private var matchFactory: LabMatchInputFactory? = null

    private var home = ""
    private var away = ""
    private var homeTactics = LabTacticalPresets.TacticalConfig()
    private var awayTactics = LabTacticalPresets.TacticalConfig()
    private var simMinutes = 20
    private var knockout = false
    private var seed = 123456789L

    private val tick = object : Runnable {
        override fun run() {
            if (isFinishing || isDestroyed || paused) return
            if (played >= narration.size) {
                val session = liveRustSession
                if (session != null && session.isOpen && !finished) {
                    executor.execute { stepLiveRustSession() }
                    return
                }
                if (finished) {
                    state.text = "Hết giờ. Chọn bất kỳ dòng nào trong danh sách để nghe lại."
                }
                return
            }
            val item = narration[played++]
            header.text = "$home ${item.homeGoals} – ${item.awayGoals} $away  (${item.displayMinute}')"
            records.add(hashMapOf("time" to item.minute, "commentary" to item.text))
            if (played % 3 == 0 || played == narration.size || item.kind == LabEventType.GOAL) {
                recordAdapter.notifyDataSetChanged()
                list.setSelection(records.size - 1)
            }
            state.text = "Phút ${item.displayMinute}'. Đã phát $played/${narration.size} câu."
            audioDirector?.onEvent(
                kind = item.kind,
                side = item.side,
                minute = item.minute,
                goalDiffAbs = kotlin.math.abs(item.homeGoals - item.awayGoals),
                nowMs = SystemClock.uptimeMillis(),
                isWoodwork = item.isWoodwork,
                isPenalty = item.isPenalty,
                isShootoutScored = item.isShootoutScored
            )
            val manager = tts
            if (manager != null && manager.isEnabled()) {
                manager.speak(item.text, Runnable { main.postDelayed(this, playbackMs) })
            } else {
                main.postDelayed(this, playbackMs)
            }
        }
    }

    private fun dp(v: Int) = (v * resources.displayMetrics.density).toInt()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        LabLogStore.init(this)
        home = intent.getStringExtra(EXTRA_HOME) ?: ""
        away = intent.getStringExtra(EXTRA_AWAY) ?: ""
        homeTactics = LabTacticalPresets.TacticalConfig(
            LabTacticalPresets.formationById(intent.getStringExtra(EXTRA_HOME_FORMATION)),
            LabTacticalPresets.styleById(intent.getStringExtra(EXTRA_HOME_STYLE))
        )
        awayTactics = LabTacticalPresets.TacticalConfig(
            LabTacticalPresets.formationById(intent.getStringExtra(EXTRA_AWAY_FORMATION)),
            LabTacticalPresets.styleById(intent.getStringExtra(EXTRA_AWAY_STYLE))
        )
        simMinutes = intent.getIntExtra(EXTRA_SIM_MINUTES, 20).coerceIn(10, 90)
        knockout = intent.getBooleanExtra(EXTRA_KNOCKOUT, false)
        seed = intent.getLongExtra(EXTRA_SEED, 123456789L)
        if (home.isBlank() || away.isBlank() || home == away) {
            finish()
            return
        }

        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(12), dp(10), dp(12), dp(12))
            setBackgroundColor(Color.rgb(16, 28, 41))
        }
        header = TextView(this).apply {
            text = "$home 0 – 0 $away  (0')"
            textSize = 21f
            setTextColor(Color.WHITE)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
                isAccessibilityHeading = true
            }
        }
        root.addView(header)
        state = TextView(this).apply {
            text = "Đang khởi chạy lõi mô phỏng Rust..."
            textSize = 15f
            setTextColor(Color.WHITE)
            setPadding(0, dp(8), 0, dp(8))
        }
        root.addView(state)

        val playbackRow = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
        playButton = Button(this).apply {
            text = "Tạm dừng"
            isAllCaps = false
            setOnClickListener {
                paused = !paused
                text = if (paused) "Tiếp tục" else "Tạm dừng"
                if (paused) {
                    main.removeCallbacks(tick)
                    tts?.stop()
                    audio?.pauseAll()
                } else {
                    audio?.resumeAll()
                    main.post(tick)
                }
            }
        }
        playbackRow.addView(playButton, LinearLayout.LayoutParams(0, dp(48), 1f))

        val speedBtn = Button(this).apply {
            text = "Tốc độ"
            isAllCaps = false
            setOnClickListener {
                playbackMs = when (playbackMs) {
                    320L -> 800L
                    800L -> 150L
                    else -> 320L
                }
                Toast.makeText(this@LabReferenceMatchActivity, "Tốc độ: ${playbackMs}ms/câu", Toast.LENGTH_SHORT).show()
            }
        }
        playbackRow.addView(speedBtn, LinearLayout.LayoutParams(0, dp(48), 1f))
        root.addView(playbackRow)

        val managerRow = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
        managerRow.addView(
            Button(this).apply {
                text = "Chiến thuật"
                isAllCaps = false
                setOnClickListener { showLiveTacticsDialog() }
            },
            LinearLayout.LayoutParams(0, dp(48), 1f)
        )
        managerRow.addView(
            Button(this).apply {
                text = "Thay người"
                isAllCaps = false
                setOnClickListener { showSubstitutionDialog() }
            },
            LinearLayout.LayoutParams(0, dp(48), 1f)
        )
        managerRow.addView(
            Button(this).apply {
                text = "Thống kê"
                isAllCaps = false
                setOnClickListener {
                    AlertDialog.Builder(this@LabReferenceMatchActivity)
                        .setTitle("Thống kê trận đấu")
                        .setMessage(statsSummaryText)
                        .setPositiveButton("Đóng", null)
                        .show()
                }
            },
            LinearLayout.LayoutParams(0, dp(48), 1f)
        )
        root.addView(managerRow)

        recordAdapter = CommentaryRecordAdapter(this, 0, records)
        list = ListView(this).apply {
            contentDescription = "Danh sách bình luận trận đấu"
            adapter = recordAdapter
        }
        root.addView(list, LinearLayout.LayoutParams(-1, 0, 1f))
        list.setOnItemClickListener { _, _, position, _ ->
            (records.getOrNull(position)?.get("commentary") as? String)?.let { tts?.speak(it) }
        }
        root.addView(
            Button(this).apply {
                text = "Kết thúc thử nghiệm và quay lại"
                isAllCaps = false
                setOnClickListener { finish() }
            },
            LinearLayout.LayoutParams(-1, dp(54))
        )
        setContentView(root)

        tts = MatchTtsManager(this)
        val audioMgr = MatchAudioManager(this, true)
        audio = audioMgr
        audioDirector = LabAudioDirector(audioMgr, neutralGround = false, stadiumCapacity = estimateStadiumCapacity(home))
        executor.execute { startMatch() }
    }

    private fun showLiveTacticsDialog() {
        val session = liveRustSession
        if (session == null || !session.isOpen || finished) {
            Toast.makeText(this, "Chỉ chỉnh chiến thuật trực tiếp khi trận Rust đang diễn ra.", Toast.LENGTH_SHORT).show()
            return
        }
        val actions = arrayOf(
            "Chủ nhà: Pressing tầm cao (85)",
            "Chủ nhà: Nhịp độ nhanh (80)",
            "Chủ nhà: Phòng ngự lùi sâu (Line 30)",
            "Chủ nhà: Giảm nhịp kiểm soát (Tempo 35)"
        )
        AlertDialog.Builder(this)
            .setTitle("Chỉ đạo trực tiếp (Rust Live Session)")
            .setItems(actions) { _, which ->
                executor.execute {
                    val ok = when (which) {
                        0 -> session.submitTeamInstruction(liveGameSecond, "HOME", "CHANGE_PRESSING", 85)
                        1 -> session.submitTeamInstruction(liveGameSecond, "HOME", "CHANGE_TEMPO", 80)
                        2 -> session.submitTeamInstruction(liveGameSecond, "HOME", "CHANGE_LINE_HEIGHT", 30)
                        else -> session.submitTeamInstruction(liveGameSecond, "HOME", "CHANGE_TEMPO", 35)
                    }
                    main.post {
                        Toast.makeText(
                            this,
                            if (ok) "Đã gửi chỉ đạo chiến thuật vào Rust!" else "Không gửi được chỉ đạo.",
                            Toast.LENGTH_SHORT
                        ).show()
                    }
                }
            }
            .setNegativeButton("Đóng", null)
            .show()
    }

    private fun showSubstitutionDialog() {
        val session = liveRustSession
        if (session == null || !session.isOpen || finished) {
            Toast.makeText(this, "Chỉ thay người trực tiếp khi trận Rust đang diễn ra.", Toast.LENGTH_SHORT).show()
            return
        }
        if (homeSubsUsed >= MAX_SUBSTITUTIONS) {
            Toast.makeText(this, "Đã dùng hết $MAX_SUBSTITUTIONS lượt thay người.", Toast.LENGTH_SHORT).show()
            return
        }
        if (homeOnPitch.isEmpty() || homeBench.isEmpty()) {
            Toast.makeText(this, "Không còn cầu thủ dự bị để thay.", Toast.LENGTH_SHORT).show()
            return
        }
        val outOptions = homeOnPitch.map { it.label }.toTypedArray()
        AlertDialog.Builder(this)
            .setTitle("Chọn cầu thủ ra sân (Còn ${MAX_SUBSTITUTIONS - homeSubsUsed} lượt)")
            .setItems(outOptions) { _, outIdx ->
                val outPlayer = homeOnPitch.getOrNull(outIdx) ?: return@setItems
                val inOptions = homeBench.map { it.label }.toTypedArray()
                AlertDialog.Builder(this)
                    .setTitle("Vào thay ${outPlayer.label}")
                    .setItems(inOptions) { _, inIdx ->
                        val inPlayer = homeBench.getOrNull(inIdx) ?: return@setItems
                        executor.execute {
                            val ok = session.submitSubstitution(liveGameSecond, "HOME", outPlayer.id, inPlayer.id)
                            main.post {
                                if (ok) {
                                    homeOnPitch[outIdx] = inPlayer
                                    homeBench.removeAt(inIdx)
                                    homeSubsUsed++
                                    Toast.makeText(
                                        this,
                                        "Đã thay người: ${outPlayer.name} → ${inPlayer.name}",
                                        Toast.LENGTH_SHORT
                                    ).show()
                                } else {
                                    Toast.makeText(this, "Không thể gửi lệnh thay người.", Toast.LENGTH_SHORT).show()
                                }
                            }
                        }
                    }
                    .setNegativeButton("Hủy", null)
                    .show()
            }
            .setNegativeButton("Đóng", null)
            .show()
    }

    private fun populateHomeRoster(inputJson: String) {
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
        main.post {
            homeOnPitch.clear()
            homeOnPitch.addAll(starters)
            homeBench.clear()
            homeBench.addAll(subs)
            homeSubsUsed = 0
        }
    }

    private fun startMatch() {
        try {
            val factory = LabMatchInputFactory(this)
            matchFactory = factory
            LabLogStore.log(
                "MATCH",
                "START $home (${homeTactics.formation.label}/${homeTactics.style.label}) vs " +
                    "$away (${awayTactics.formation.label}/${awayTactics.style.label}) | " +
                    "rustReady=${NativeBridge.isFullEngineReady()} simMin=$simMinutes knockout=$knockout seed=$seed"
            )
            if (!NativeBridge.isFullEngineReady()) {
                throw IllegalStateException("Thư viện lõi Rust (libfootball_match_engine.so) chưa được biên dịch/đóng gói vào APK.")
            }
            val inputJson = factory.createRustInputJson(home, away, seed, simMinutes, knockout, homeTactics, awayTactics)
            populateHomeRoster(inputJson)
            val session = LabRustLiveSession.open(inputJson)
                ?: throw IllegalStateException("Không tạo được phiên Rust MatchSession")
            liveRustSession = session
            rustCommentary = LabCommentaryEngine(this, home, away, seed)
            stepLiveRustSession()
        } catch (error: Exception) {
            LabLogStore.error("ERROR", "startMatch failed", error)
            main.post {
                finished = true
                state.text = "Không chạy được trận đấu: ${error.javaClass.simpleName}: ${error.message}"
            }
        }
    }

    private fun stepLiveRustSession() {
        val session = liveRustSession ?: return
        val factory = matchFactory ?: return
        val engine = rustCommentary ?: return
        try {
            val added = ArrayList<LabNarration>()
            var guard = 0
            while (added.isEmpty() && !finished && guard++ < 40) {
                val stepObj = session.advance(maxSteps = 8)
                if (!stepObj.optBoolean("ok", false)) {
                    throw IllegalStateException(stepObj.optString("error", "Rust step failed"))
                }
                liveGameSecond = stepObj.optInt("gameSecond", liveGameSecond)
                val eventsArr = stepObj.optJSONArray("events")
                if (eventsArr != null) {
                    for (i in 0 until eventsArr.length()) {
                        val ev = eventsArr.getJSONObject(i)
                        val kind = try {
                            LabEventType.valueOf(ev.getString("type"))
                        } catch (_: IllegalArgumentException) {
                            continue
                        }
                        val side = when (ev.optString("side", "")) {
                            "HOME" -> LabTeamSide.HOME
                            "AWAY" -> LabTeamSide.AWAY
                            else -> null
                        }
                        val minute = ev.optInt("minute", (ev.optInt("gameSecond") + 59) / 60).coerceAtLeast(1)
                        val displayMin = ev.optString("displayMinute", minute.toString())
                        val hg = ev.optInt("homeGoals", 0)
                        val ag = ev.optInt("awayGoals", 0)
                        val actorName = factory.name(ev.optString("actor", "").takeIf { it != "null" })
                        val targetName = factory.name(ev.optString("target", "").takeIf { it != "null" })
                        val text = engine.renderRustEvent(ev, kind, side, actorName, targetName, minute, hg, ag)
                            ?: continue
                        val metaObj = ev.optJSONObject("metadata")
                        val isWoodwork = metaObj?.optString("woodwork") == "true" ||
                            metaObj?.optString("outcome")?.equals("WOODWORK", ignoreCase = true) == true
                        val isPenalty = metaObj?.optString("source")?.equals("PENALTY", ignoreCase = true) == true ||
                            kind == LabEventType.PENALTY_KICK
                        val shootoutScored = if (kind == LabEventType.PENALTY_SHOOTOUT_KICK) {
                            metaObj?.optString("scored") == "true" || metaObj?.optString("outcome")?.equals("SCORED", ignoreCase = true) == true
                        } else null
                        LabLogStore.log("EVENT", "${displayMin}' [$kind] side=${side ?: "-"} actor=$actorName target=$targetName score=$hg-$ag wood=$isWoodwork pen=$isPenalty | $text")
                        added.add(LabNarration(minute, displayMin, hg, ag, text, kind, side, isWoodwork, isPenalty, shootoutScored))
                    }
                }
                if (stepObj.optBoolean("fullTime", false)) {
                    val finalObj = session.finish()
                    updateStatsFromRustJson(finalObj)
                    session.close()
                    finished = true
                    break
                }
            }
            main.post {
                narration.addAll(added)
                if (added.isNotEmpty() || finished) {
                    main.post(tick)
                }
            }
        } catch (error: Exception) {
            LabLogStore.error("ERROR", "stepLiveRustSession failed", error)
            main.post {
                finished = true
                state.text = "Lỗi phiên Rust: ${error.message}"
            }
        }
    }

    private fun updateStatsFromRustJson(root: JSONObject) {
        val stats = root.optJSONObject("statistics")
        val hs = stats?.optJSONObject("home")
        val asObj = stats?.optJSONObject("away")
        statsSummaryText = buildString {
            appendLine("Động cơ: Rust Live Session | Seed: $seed")
            appendLine("Tỷ số chung cuộc: $home ${root.optInt("homeGoals")} – ${root.optInt("awayGoals")} $away")
            if (hs != null && asObj != null) {
                appendLine("Kiểm soát bóng: ${hs.optDouble("possessionPercent")}% – ${asObj.optDouble("possessionPercent")}%")
                appendLine("Sút (trúng đích): ${hs.optInt("shots")} (${hs.optInt("shotsOnTarget")}) – ${asObj.optInt("shots")} (${asObj.optInt("shotsOnTarget")})")
                appendLine("xG: ${"%.2f".format(hs.optInt("expectedGoalsMilli") / 1000.0)} – ${"%.2f".format(asObj.optInt("expectedGoalsMilli") / 1000.0)}")
                appendLine("Chuyền thành công: ${hs.optInt("passesCompleted")}/${hs.optInt("passesAttempted")} – ${asObj.optInt("passesCompleted")}/${asObj.optInt("passesAttempted")}")
                appendLine("Phạt góc: ${hs.optInt("corners")} – ${asObj.optInt("corners")} | Phạm lỗi: ${hs.optInt("fouls")} – ${asObj.optInt("fouls")}")
                appendLine("Thẻ vàng/đỏ: ${hs.optInt("yellowCards")}/${hs.optInt("redCards")} – ${asObj.optInt("yellowCards")}/${asObj.optInt("redCards")}")
            }
            val mom = root.optJSONObject("manOfTheMatch")
            if (mom != null) {
                val rStr = "%.1f".format(mom.optInt("ratingTenths", 62) / 10.0)
                appendLine("Cầu thủ xuất sắc nhất: ${mom.optString("displayName")} ($rStr) — ${mom.optInt("goals")} bàn, ${mom.optInt("assists")} kiến tạo")
            }
            val ratings = root.optJSONArray("playerRatings")
            if (ratings != null && ratings.length() > 0) {
                appendLine("\nChấm điểm nổi bật:")
                for (i in 0 until minOf(6, ratings.length())) {
                    val pr = ratings.getJSONObject(i)
                    val rStr = "%.1f".format(pr.optInt("ratingTenths", 62) / 10.0)
                    val teamTag = if (pr.optString("side") == "HOME") "Nhà" else "Khách"
                    appendLine("  • ${pr.optString("displayName")} ($teamTag, ${pr.optString("primaryPosition")}): $rStr (${pr.optInt("goals")}B/${pr.optInt("assists")}KT)")
                }
            }
            appendLine("\nTổng sự kiện: ${root.optInt("eventCount")} | FP: ${root.optLong("fingerprint")}")
        }
        LabLogStore.log("MATCH", "FINAL STATS:\n$statsSummaryText")
    }

    private fun estimateStadiumCapacity(clubName: String): Int {
        val lower = clubName.lowercase()
        return when {
            lower.contains("manchester united") || lower.contains("arsenal") ||
                lower.contains("tottenham") || lower.contains("west ham") ||
                lower.contains("liverpool") || lower.contains("manchester city") ||
                lower.contains("newcastle") -> 58000
            lower.contains("bournemouth") || lower.contains("luton") ||
                lower.contains("brentford") || lower.contains("burnley") ||
                lower.contains("fulham") -> 16000
            else -> 34000
        }
    }

    override fun onDestroy() {
        main.removeCallbacks(tick)
        executor.shutdownNow()
        liveRustSession?.close()
        tts?.release()
        audio?.release()
        super.onDestroy()
    }
}
