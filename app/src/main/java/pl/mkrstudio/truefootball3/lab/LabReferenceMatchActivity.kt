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
import org.json.JSONArray
import org.json.JSONObject
import pl.mkrstudio.truefootball3.adapters.CommentaryRecordAdapter
import pl.mkrstudio.truefootball3.audio.MatchAudioManager
import pl.mkrstudio.truefootball3.audio.MatchTtsManager
import pl.mkrstudio.truefootball3.audio.TtsSettingsStore
import java.util.ArrayDeque
import java.util.ArrayList
import java.util.HashMap
import java.util.concurrent.Executors
import kotlin.math.roundToInt
import kotlin.math.roundToLong

/**
 * Unified match screen shared identically by both Match Engine 3.0 and Match Engine 4.0.
 * Honors the synchronized viewing mode (Full trận / Chi tiết / Ngắn gọn) and TTS speed settings.
 */
class LabReferenceMatchActivity : Activity() {
    companion object {
        const val EXTRA_HOME = "home"
        const val EXTRA_AWAY = "away"
        const val EXTRA_HOME_FORMATION = "home_formation"
        const val EXTRA_HOME_STYLE = "home_style"
        const val EXTRA_AWAY_FORMATION = "away_formation"
        const val EXTRA_AWAY_STYLE = "away_style"
        const val EXTRA_SIM_MINUTES = "sim_minutes"
        const val EXTRA_VIEWING_MODE = "viewing_mode"
        const val EXTRA_ENGINE_VERSION = "engine_version"
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
        val isShootoutScored: Boolean? = null,
        val simulationBeat: Long = 0L,
        val gameSecond: Int = 0
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
    private val homeRosterIndex = HashMap<String, RosterEntry>()
    private var homeSubsUsed = 0
    private var homeWindowsUsed = 0
    private var intervalSubstitutionAllowed = false
    private var commandDialogActive = false
    private var simulationStepPending = false
    private var simulatedV4Beat = 0L
    private var presentedV4Beat = 0L
    private var presentedV4GameSecond = 0
    private data class DeferredV4Command(
        val description: String,
        val execute: (LabRustLiveSession) -> Boolean,
        val onComplete: (Boolean) -> Unit
    )
    private val pendingV4Commands = ArrayDeque<DeferredV4Command>()
    private var played = 0
    private var paused = false
    private var playbackMs = 320L
    private var manualSpeedOverride = false
    private var finished = false
    private var liveGameSecond = 0
    private var lastV4PlayingPeriod = 1
    private var runningHomeGoals = 0
    private var runningAwayGoals = 0
    private var v4InputJson = ""
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
    private var simMinutes = 90
    private var viewingMode = LabViewingMode.FULL_MATCH
    private var engineVersion = LabEngineVersion.V4
    private var includeTurningPointsInShort = true
    private var knockout = false
    private var seed = 123456789L

    private fun currentStepDelayMs(): Long {
        if (manualSpeedOverride) return playbackMs
        val simSettings = SimulationSettingsStore(this).load()
        if (!simSettings.syncMatchSpeedWithTts) return playbackMs
        val rate = (TtsSettingsStore(this).load().ratePercent / 100f).coerceIn(0.5f, 5.0f)
        return (playbackMs / rate).roundToLong().coerceAtLeast(100L)
    }

    private val tick = object : Runnable {
        override fun run() {
            if (isFinishing || isDestroyed || paused || commandDialogActive) return
            if (played >= narration.size) {
                val session = liveRustSession
                if (engineVersion == LabEngineVersion.V4 && !finished &&
                    pendingV4Commands.isNotEmpty() && !simulationStepPending && session != null && session.isOpen) {
                    // Accepted only at the PRESENTATION checkpoint: no unseen
                    // action beats or queued narration can be jumped over.
                    if (presentedV4Beat != simulatedV4Beat) return
                    val command = pendingV4Commands.removeFirst()
                    simulationStepPending = true
                    executor.execute {
                        val ok = try { command.execute(session) } catch (e: Exception) {
                            LabLogStore.error("CMD", "V4 checkpoint command failed", e)
                            false
                        }
                        main.post {
                            simulationStepPending = false
                            if (!isFinishing && !isDestroyed) {
                                command.onComplete(ok)
                                if (!paused && !commandDialogActive) main.post(this)
                            }
                        }
                    }
                    return
                }
                if (session != null && session.isOpen && !finished && !simulationStepPending) {
                    simulationStepPending = true
                    executor.execute { stepLiveRustSession() }
                    return
                }
                if (finished) {
                    state.text = "Hết giờ. Chọn bất kỳ dòng nào trong danh sách để nghe lại."
                }
                return
            }
            val item = narration[played++]
            if (engineVersion == LabEngineVersion.V4) {
                presentedV4Beat = item.simulationBeat
                presentedV4GameSecond = item.gameSecond
            }
            header.text = "$home ${item.homeGoals} – ${item.awayGoals} $away  (${item.displayMinute}')"
            records.add(hashMapOf("time" to item.minute, "commentary" to item.text))
            if (played % 3 == 0 || played == narration.size || item.kind == LabEventType.GOAL) {
                recordAdapter.notifyDataSetChanged()
                list.setSelection(records.size - 1)
            }
            state.text = "Phút ${item.displayMinute}' • ${engineVersion.labelVi} • ${viewingMode.labelVi} ($played/${narration.size})"
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
            val delayMs = currentStepDelayMs()
            val manager = tts
            if (manager != null && manager.isEnabled()) {
                manager.speak(item.text, Runnable { main.postDelayed(this, delayMs) })
            } else {
                main.postDelayed(this, delayMs)
            }
        }
    }

    private fun dp(v: Int) = (v * resources.displayMetrics.density).toInt()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        LabLogStore.init(this)
        val savedSim = SimulationSettingsStore(this).load()
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
        simMinutes = intent.getIntExtra(EXTRA_SIM_MINUTES, 90).coerceIn(10, 90)
        viewingMode = intent.getStringExtra(EXTRA_VIEWING_MODE)?.let {
            try { LabViewingMode.valueOf(it) } catch (_: IllegalArgumentException) { null }
        } ?: savedSim.viewingMode
        engineVersion = intent.getStringExtra(EXTRA_ENGINE_VERSION)?.let {
            try { LabEngineVersion.valueOf(it) } catch (_: IllegalArgumentException) { null }
        } ?: savedSim.engineVersion
        includeTurningPointsInShort = savedSim.includeTurningPointsInShort
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
            text = "Đang khởi chạy lõi mô phỏng Rust ${engineVersion.labelVi} (${viewingMode.labelVi})..."
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
                manualSpeedOverride = true
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

    private fun openCommandDialog() {
        commandDialogActive = true
        main.removeCallbacks(tick)
    }

    private fun closeCommandDialog() {
        commandDialogActive = false
        if (!paused && !isFinishing && !isDestroyed) main.post(tick)
    }

    private fun enqueueV4Command(command: DeferredV4Command) {
        // Never submit to Rust while a more recent, unseen beat is buffered.
        // A command is applied when the current narration segment has finished.
        pendingV4Commands.addLast(command)
        state.text = "Đã nhận lệnh: ${command.description}. Áp dụng sau tình huống đang phát."
        closeCommandDialog()
    }

    private fun showLiveTacticsDialog() {
        val session = liveRustSession
        if (session == null || !session.isOpen || finished) {
            Toast.makeText(this, "Chỉ chỉnh chiến thuật khi trận Rust đang diễn ra.", Toast.LENGTH_SHORT).show()
            return
        }
        openCommandDialog()
        val actions = arrayOf(
            "Chủ nhà: Pressing tầm cao (85)",
            "Chủ nhà: Nhịp độ nhanh (80)",
            "Chủ nhà: Phòng ngự lùi sâu (Line 30)",
            "Chủ nhà: Giảm nhịp kiểm soát (Tempo 35)"
        )
        AlertDialog.Builder(this)
            .setTitle("Chỉ đạo trực tiếp (Rust ${engineVersion.labelVi})")
            .setItems(actions) { _, which ->
                val type = when (which) {
                    0 -> "CHANGE_PRESSING"
                    1, 3 -> "CHANGE_TEMPO"
                    else -> "CHANGE_LINE_HEIGHT"
                }
                val value = when (which) { 0 -> 85; 1 -> 80; 2 -> 30; else -> 35 }
                if (engineVersion == LabEngineVersion.V4) {
                    enqueueV4Command(DeferredV4Command(
                        description = actions[which],
                        execute = { s -> s.submitTeamInstruction(presentedV4GameSecond, "HOME", type, value) },
                        onComplete = { ok ->
                            Toast.makeText(this, if (ok) "Đã áp dụng chiến thuật" else "Rust từ chối lệnh chiến thuật", Toast.LENGTH_SHORT).show()
                        }
                    ))
                } else {
                    closeCommandDialog()
                    executor.execute {
                        val ok = session.submitTeamInstruction(liveGameSecond, "HOME", type, value)
                        main.post { if (!isDestroyed) Toast.makeText(this,
                            if (ok) "Đã gửi chỉ đạo" else "Không gửi được chỉ đạo",
                            Toast.LENGTH_SHORT).show() }
                    }
                }
            }
            .setNegativeButton("Đóng") { _, _ -> closeCommandDialog() }
            .setOnCancelListener { closeCommandDialog() }
            .show()
    }

    private fun showSubstitutionDialog() {
        if (engineVersion == LabEngineVersion.V4) {
            showV4BatchSubstitutionDialog()
            return
        }
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

    private fun showV4BatchSubstitutionDialog() {
        val session = liveRustSession
        if (session == null || !session.isOpen || finished) {
            Toast.makeText(this, "Phiên Rust V4 không còn hoạt động", Toast.LENGTH_SHORT).show()
            return
        }
        val remaining = MAX_SUBSTITUTIONS - homeSubsUsed
        if (remaining <= 0 || (!intervalSubstitutionAllowed && homeWindowsUsed >= 3)) {
            Toast.makeText(this, "Đã hết suất hoặc cơ hội thay người hợp lệ", Toast.LENGTH_SHORT).show()
            return
        }
        if (homeOnPitch.isEmpty() || homeBench.isEmpty()) {
            Toast.makeText(this, "Không còn cầu thủ hợp lệ để thay", Toast.LENGTH_SHORT).show()
            return
        }
        openCommandDialog()
        val candidates = homeOnPitch.toList()
        val bench = homeBench.toList()
        val checked = BooleanArray(candidates.size)
        val title = "Chọn tối đa $remaining cầu thủ ra sân (đã dùng $homeWindowsUsed/3 cơ hội)"
        AlertDialog.Builder(this)
            .setTitle(title)
            .setMultiChoiceItems(candidates.map { it.label }.toTypedArray(), checked) { _, which, enabled ->
                checked[which] = enabled
            }
            .setPositiveButton("Chọn người vào") { _, _ ->
                val selected = candidates.indices.filter { checked[it] }.map { candidates[it] }
                if (selected.isEmpty() || selected.size > remaining || selected.size > bench.size) {
                    Toast.makeText(this, "Chọn từ 1 đến $remaining người (không quá số dự bị)", Toast.LENGTH_LONG).show()
                    closeCommandDialog()
                } else {
                    chooseV4IncomingPlayers(selected, bench.toMutableList(), ArrayList())
                }
            }
            .setNegativeButton("Hủy") { _, _ -> closeCommandDialog() }
            .setOnCancelListener { closeCommandDialog() }
            .show()
    }

    private fun chooseV4IncomingPlayers(
        outgoing: List<RosterEntry>,
        available: MutableList<RosterEntry>,
        pairs: ArrayList<Pair<RosterEntry, RosterEntry>>
    ) {
        if (pairs.size == outgoing.size) {
            val text = pairs.joinToString("\n") { "${it.first.name} → ${it.second.name}" }
            AlertDialog.Builder(this)
                .setTitle("Xác nhận thay ${pairs.size} cầu thủ trong 1 cơ hội")
                .setMessage(text)
                .setPositiveButton("Thực hiện") { _, _ ->
                    val chosen = pairs.toList()
                    enqueueV4Command(DeferredV4Command(
                        description = "Thay ${chosen.size} cầu thủ",
                        execute = { s -> s.submitSubstitutions(
                            presentedV4GameSecond, "HOME", chosen.map { it.first.id to it.second.id }
                        ) },
                        onComplete = { ok ->
                            if (ok) {
                                for ((out, incoming) in chosen) {
                                    homeOnPitch.removeAll { it.id == out.id }
                                    homeOnPitch.add(incoming)
                                    homeBench.removeAll { it.id == incoming.id }
                                }
                                homeSubsUsed += chosen.size
                                if (!intervalSubstitutionAllowed) homeWindowsUsed++
                            }
                            Toast.makeText(this,
                                if (ok) "Rust đã thay ${chosen.size} cầu thủ" else "Rust từ chối nhóm thay người; đội hình không thay đổi",
                                Toast.LENGTH_LONG).show()
                        }
                    ))
                }
                .setNegativeButton("Hủy") { _, _ -> closeCommandDialog() }
                .setOnCancelListener { closeCommandDialog() }
                .show()
            return
        }
        val idx = pairs.size
        AlertDialog.Builder(this)
            .setTitle("Chọn người vào thay ${outgoing[idx].name}")
            .setItems(available.map { it.label }.toTypedArray()) { _, selected ->
                val entry = available.removeAt(selected)
                pairs.add(outgoing[idx] to entry)
                chooseV4IncomingPlayers(outgoing, available, pairs)
            }
            .setNegativeButton("Hủy") { _, _ -> closeCommandDialog() }
            .setOnCancelListener { closeCommandDialog() }
            .show()
    }

    /** Never trust the app's cached XI after an AI manager has substituted. */
    private fun syncV4HomeRoster(authoritative: JSONObject) {
        fun getPlayers(field: String): List<RosterEntry> {
            val ids = authoritative.optJSONArray(field) ?: return emptyList()
            return (0 until ids.length()).mapNotNull { idx ->
                val id = ids.optString(idx)
                homeRosterIndex[id]
            }
        }
        homeOnPitch.clear()
        homeOnPitch.addAll(getPlayers("onPitchPlayerIds"))
        homeBench.clear()
        homeBench.addAll(getPlayers("availableBenchPlayerIds"))
        homeSubsUsed = authoritative.optInt("substitutionsUsed", homeSubsUsed)
        homeWindowsUsed = authoritative.optInt("substitutionWindowsUsed", homeWindowsUsed)
        intervalSubstitutionAllowed = authoritative.optBoolean("intervalSubstitutionAllowed", false)
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
            homeRosterIndex.clear()
            for (entry in starters + subs) homeRosterIndex[entry.id] = entry
            homeOnPitch.clear()
            homeOnPitch.addAll(starters)
            homeBench.clear()
            homeBench.addAll(subs)
            homeSubsUsed = 0
            homeWindowsUsed = 0
            intervalSubstitutionAllowed = false
        }
    }

    private fun startMatch() {
        try {
            val factory = LabMatchInputFactory(this)
            matchFactory = factory
            runningHomeGoals = 0
            runningAwayGoals = 0
            LabLogStore.log(
                "MATCH",
                "START [$engineVersion/$viewingMode] $home (${homeTactics.formation.label}/${homeTactics.style.label}) vs " +
                    "$away (${awayTactics.formation.label}/${awayTactics.style.label}) | knockout=$knockout seed=$seed"
            )
            val inputJson = when (engineVersion) {
                LabEngineVersion.V4 -> {
                    if (!NativeBridge.isV4InteractiveReady()) {
                        throw IllegalStateException("Thư viện lõi Rust V4 tương tác (bridge v6) chưa được đóng gói trong APK.")
                    }
                    factory.createRustV4InputJson(
                        homeName = home,
                        awayName = away,
                        seed = seed,
                        knockout = knockout,
                        simulationMode = LabSimulationMode.FULL_EVENT,
                        homeTactics = homeTactics,
                        awayTactics = awayTactics
                    ).also { v4InputJson = it }
                }
                LabEngineVersion.V3 -> {
                    if (!NativeBridge.isFullEngineReady()) {
                        throw IllegalStateException("Thư viện lõi Rust V3 (libfootball_match_engine.so) chưa được biên dịch/đóng gói vào APK.")
                    }
                    factory.createRustInputJson(home, away, seed, simMinutes, knockout, homeTactics, awayTactics)
                }
            }
            populateHomeRoster(inputJson)
            val session = LabRustLiveSession.open(inputJson, engineVersion)
                ?: throw IllegalStateException("Không tạo được phiên Rust ${engineVersion.labelVi}")
            if (isFinishing || isDestroyed) {
                session.close()
                return
            }
            liveRustSession = session
            simulationStepPending = true
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

    private fun shouldIncludeV3Event(kind: LabEventType): Boolean {
        return when (viewingMode) {
            LabViewingMode.FULL_MATCH -> true
            LabViewingMode.EXTENDED_HIGHLIGHTS -> kind in setOf(
                LabEventType.MATCH_STARTED, LabEventType.HALF_TIME, LabEventType.FULL_TIME,
                LabEventType.EXTRA_TIME_STARTED, LabEventType.EXTRA_TIME_HALF_TIME,
                LabEventType.PENALTY_SHOOTOUT_STARTED, LabEventType.PENALTY_SHOOTOUT_KICK, LabEventType.PENALTY_SHOOTOUT_ENDED,
                LabEventType.GOAL, LabEventType.OWN_GOAL, LabEventType.SHOT, LabEventType.SHOT_ON_TARGET,
                LabEventType.SHOT_OFF_TARGET, LabEventType.HEADER, LabEventType.SAVE, LabEventType.REBOUND,
                LabEventType.CORNER, LabEventType.FREE_KICK, LabEventType.PENALTY_AWARDED, LabEventType.PENALTY_KICK,
                LabEventType.YELLOW_CARD, LabEventType.RED_CARD, LabEventType.INJURY, LabEventType.SUBSTITUTION,
                LabEventType.OFFSIDE
            )
            LabViewingMode.SHORT_HIGHLIGHTS -> kind in setOf(
                LabEventType.MATCH_STARTED, LabEventType.HALF_TIME, LabEventType.FULL_TIME,
                LabEventType.EXTRA_TIME_STARTED, LabEventType.PENALTY_SHOOTOUT_STARTED,
                LabEventType.PENALTY_SHOOTOUT_KICK, LabEventType.PENALTY_SHOOTOUT_ENDED,
                LabEventType.GOAL, LabEventType.OWN_GOAL
            ) || (includeTurningPointsInShort && kind in setOf(
                LabEventType.PENALTY_AWARDED, LabEventType.PENALTY_KICK, LabEventType.RED_CARD
            ))
        }
    }

    /** Display FIFA-style regulation minutes using period-local elapsed time.
     *  gameSecond is only elapsed wall-simulation time (including prior added
     *  time) and must never be used to infer "45+"/"90+" labels in V4. */
    private fun formatDisplayMinute(
        period: Int,
        gameSecond: Int,
        periodElapsedSecond: Int? = null
    ): String {
        if (engineVersion == LabEngineVersion.V4 && periodElapsedSecond != null) {
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
        // Legacy V3 timestamps remain supported without changing V3 behavior.
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

    private fun stepLiveRustSession() {
        val session = liveRustSession ?: return
        val factory = matchFactory ?: return
        val engine = rustCommentary ?: return
        try {
            val added = ArrayList<LabNarration>()
            var latestRosterSnapshot: JSONObject? = null
            var guard = 0
            // One Rust V4 action beat at a time: never simulate another beat
            // while the listener is still hearing events from the preceding one.
            val maxEmptySteps = if (engineVersion == LabEngineVersion.V4) 1 else 80
            while (added.isEmpty() && !finished && guard++ < maxEmptySteps) {
                val stepObj = session.advance(maxSteps = if (engineVersion == LabEngineVersion.V4) 1 else 8)
                if (!stepObj.optBoolean("ok", false)) {
                    throw IllegalStateException(stepObj.optString("error", "Rust step failed"))
                }
                liveGameSecond = stepObj.optInt("gameSecond", liveGameSecond)
                val committedBeat = stepObj.optLong("completedBeats", 0L)
                val liveRoster = if (engineVersion == LabEngineVersion.V4) {
                    stepObj.optJSONObject("homeRoster")?.let { JSONObject(it.toString()) }
                } else null
                if (engineVersion == LabEngineVersion.V4) {
                    latestRosterSnapshot = liveRoster
                    simulatedV4Beat = committedBeat
                    val rawEvents = stepObj.optJSONArray("newEvents") ?: JSONArray()
                    val filtered = LabV4PlaybackCoordinator.filterEventsForViewingMode(
                        rawEvents,
                        viewingMode,
                        includeTurningPointsInShort
                    )
                    val filteredSeqs = HashSet<Int>(filtered.size)
                    for (fe in filtered) {
                        filteredSeqs.add(fe.optInt("sequence", -1))
                    }
                    for (i in 0 until rawEvents.length()) {
                        val ev = rawEvents.getJSONObject(i)
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
                        if (kind == LabEventType.GOAL) {
                            if (side == LabTeamSide.HOME) runningHomeGoals++
                            else if (side == LabTeamSide.AWAY) runningAwayGoals++
                        } else if (kind == LabEventType.OWN_GOAL) {
                            if (side == LabTeamSide.HOME) runningAwayGoals++
                            else if (side == LabTeamSide.AWAY) runningHomeGoals++
                        }
                        val seq = ev.optInt("sequence", -1)
                        if (viewingMode != LabViewingMode.FULL_MATCH && seq !in filteredSeqs) {
                            continue
                        }
                        val period = ev.optInt("period", stepObj.optInt("period", 1))
                        if (period in 1..4) lastV4PlayingPeriod = period
                        val evSec = ev.optInt("gameSecond", liveGameSecond)
                        val periodElapsed = ev.optInt("periodElapsedSecond", -1).takeIf { it >= 0 }
                        val minuteInPeriod = if (periodElapsed != null) {
                            ((periodElapsed + 59) / 60).coerceAtLeast(0)
                        } else ((evSec + 59) / 60).coerceAtLeast(1)
                        val minute = when (period) {
                            2 -> 45 + minuteInPeriod
                            3 -> 90 + minuteInPeriod
                            4 -> 105 + minuteInPeriod
                            5, 6 -> if (lastV4PlayingPeriod >= 3) 120 else 90
                            else -> minuteInPeriod
                        }.coerceAtLeast(1)
                        val displayMin = formatDisplayMinute(period, evSec, periodElapsed)
                        val actorName = factory.name(ev.optString("actor", "").takeIf { it != "null" })
                        val targetName = factory.name(ev.optString("target", "").takeIf { it != "null" })

                        val normalized = JSONObject(ev.toString())
                        val ballPos = ev.optJSONObject("ballPos")
                        if (ballPos != null) {
                            val bx = ((ballPos.optDouble("x", 52.5) / 105.0) * 100.0).roundToInt().coerceIn(0, 100)
                            normalized.put("ballX", bx)
                        }
                        val text = engine.renderRustEvent(
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
                            "${displayMin}' [V4:$kind] side=${side ?: "-"} actor=$actorName target=$targetName score=$runningHomeGoals-$runningAwayGoals | $text"
                        )
                        added.add(
                            LabNarration(
                                minute, displayMin, runningHomeGoals, runningAwayGoals,
                                text, kind, side, isWoodwork, isPenalty, shootoutScored,
                                simulationBeat = committedBeat, gameSecond = evSec
                            )
                        )
                    }
                    // Keep authoritative score synced with Rust step summary
                    runningHomeGoals = stepObj.optInt("homeGoals", runningHomeGoals)
                    runningAwayGoals = stepObj.optInt("awayGoals", runningAwayGoals)

                    if (stepObj.optBoolean("isFullTime", false)) {
                        val finalObj = session.finish()
                        updateStatsFromV4Json(finalObj)
                        saveV4SeedReplay(finalObj)
                        session.close()
                        finished = true
                        break
                    }
                } else {
                    val eventsArr = stepObj.optJSONArray("events")
                    if (eventsArr != null) {
                        for (i in 0 until eventsArr.length()) {
                            val ev = eventsArr.getJSONObject(i)
                            val kind = try {
                                LabEventType.valueOf(ev.getString("type"))
                            } catch (_: IllegalArgumentException) {
                                continue
                            }
                            if (!shouldIncludeV3Event(kind)) continue
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
                                metaObj?.optString("scored") == "true" ||
                                    metaObj?.optString("outcome")?.equals("SCORED", ignoreCase = true) == true
                            } else null
                            LabLogStore.log("EVENT", "${displayMin}' [V3:$kind] side=${side ?: "-"} actor=$actorName target=$targetName score=$hg-$ag | $text")
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
            }
            main.post {
                simulationStepPending = false
                if (isFinishing || isDestroyed) return@post
                // Delivered on UI thread *before* any further session advancement.
                if (engineVersion == LabEngineVersion.V4) {
                    latestRosterSnapshot?.let { syncV4HomeRoster(it) }
                }
                narration.addAll(added)
                if (engineVersion == LabEngineVersion.V4 && added.isEmpty()) {
                    presentedV4Beat = simulatedV4Beat
                    presentedV4GameSecond = liveGameSecond
                }
                if (added.isNotEmpty() || finished) main.post(tick)
                else main.postDelayed(tick, currentStepDelayMs())
            }
        } catch (error: Exception) {
            LabLogStore.error("ERROR", "stepLiveRustSession failed", error)
            main.post {
                simulationStepPending = false
                finished = true
                state.text = "Lỗi phiên Rust: ${error.message}"
            }
        }
    }

    private fun saveV4SeedReplay(finalObj: JSONObject) {
        if (!finalObj.optBoolean("ok", false) || v4InputJson.isBlank()) return
        try {
            val matchId = finalObj.optString("matchId", "V4-$seed")
            LabGlobalSeedReplayStore(this).saveSnapshot(
                LabGlobalSeedReplayStore.StoredMatchRecord(
                    matchId = matchId,
                    seed = seed,
                    engineVersion = finalObj.optString("engineVersion", LabMatchInputFactory.ENGINE_VERSION_V4),
                    inputJson = v4InputJson,
                    summaryJson = finalObj.optJSONObject("summary")?.toString() ?: "{}",
                    commandTimelineJson = finalObj.optJSONArray("commandTimeline")?.toString() ?: "[]",
                    eventTimelineFingerprint = finalObj.optString("eventTimelineFingerprint", "")
                )
            )
        } catch (e: Exception) {
            LabLogStore.error("SEED_REPLAY", "Lưu Seed-Replay thất bại", e)
        }
    }

    private fun updateStatsFromV4Json(root: JSONObject) {
        val summary = root.optJSONObject("summary")
        val hs = summary?.optJSONObject("home")
        val asObj = summary?.optJSONObject("away")
        val factory = matchFactory
        statsSummaryText = buildString {
            appendLine("Động cơ: Rust Engine 4.0 (${viewingMode.labelVi}) | Seed: $seed")
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
                    val pName = factory?.name(p.optString("playerId")) ?: p.optString("playerId")
                    val teamTag = if (p.optString("side") == "HOME") "Nhà" else "Khách"
                    val rStr = "%.1f".format(p.optDouble("rating", 6.0))
                    val distKm = "%.1f".format(p.optDouble("distanceM", 0.0) / 1000.0)
                    val ownGoalTag = if (p.optInt("ownGoals") > 0) ", ${p.optInt("ownGoals")} phản lưới" else ""
                    appendLine("  • $pName ($teamTag): $rStr (${p.optInt("goals")}B/${p.optInt("assists")}KT$ownGoalTag • ${distKm}km)")
                }
            }
            appendLine("\nTổng sự kiện: ${root.optInt("eventCount")} | FP: ${root.optString("eventTimelineFingerprint")}")
        }
        LabLogStore.log("MATCH", "FINAL V4 STATS:\n$statsSummaryText")
    }

    private fun updateStatsFromRustJson(root: JSONObject) {
        val stats = root.optJSONObject("statistics")
        val hs = stats?.optJSONObject("home")
        val asObj = stats?.optJSONObject("away")
        statsSummaryText = buildString {
            appendLine("Động cơ: Rust Engine 3.0 (${viewingMode.labelVi}) | Seed: $seed")
            appendLine("Tỷ số chung cuộc: $home ${root.optInt("homeGoals")} – ${root.optInt("awayGoals")} $away")
            if (hs != null && asObj != null) {
                appendLine("Kiểm soát bóng: ${hs.optDouble("possessionPercent")}% – ${asObj.optDouble("possessionPercent")}%")
                appendLine("Sút (trúng đích): ${hs.optInt("shots")} (${hs.optInt("shotsOnTarget")}) – ${asObj.optInt("shots")} (${asObj.optInt("shotsOnTarget")})")
                appendLine("Sút chệch/bị chặn: ${hs.optInt("shotsOffTarget")}/${hs.optInt("shotsBlocked")} – ${asObj.optInt("shotsOffTarget")}/${asObj.optInt("shotsBlocked")}")
                appendLine("Cứu thua: ${hs.optInt("saves")} – ${asObj.optInt("saves")} | Phản lưới nhà: ${hs.optInt("ownGoalsConceded")} – ${asObj.optInt("ownGoalsConceded")}")
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
        LabLogStore.log("MATCH", "FINAL V3 STATS:\n$statsSummaryText")
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
        pendingV4Commands.clear()
        commandDialogActive = false
        main.removeCallbacks(tick)
        executor.shutdownNow()
        liveRustSession?.close()
        tts?.release()
        audio?.release()
        super.onDestroy()
    }
}
