package pl.mkrstudio.truefootball3.lab

import android.app.Activity
import android.app.AlertDialog
import android.graphics.Color
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.view.View
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
import java.util.ArrayList
import java.util.HashMap
import java.util.concurrent.Executors
import pl.mkrstudio.truefootball3.presentation.PresentationSessionGate
import kotlin.math.abs
import kotlin.math.roundToLong

/**
 * Unified match screen shared by both Match Engine 3.0 (legacy tick) and Match Engine 4.0 DES v1.4
 * ([MatchPlaybackController] + [NarrationScheduler] + [MatchCheckpointStore]).
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
        val gameSecond: Int = 0,
        val simTimeMs: Long = gameSecond * 1000L
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
    private val sessionGate = PresentationSessionGate()
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

    // Legacy V3 playback state
    private var played = 0
    private var paused = false
    private var finished = false
    private var cueAdapter = MatchEventCueAdapter()
    private var v4InputJson = ""
    private var v4MatchId = ""
    private var statsSummaryText = "Chưa có thống kê trận đấu."

    // DES v1.4 V4 playback state
    private var desRateIndex = 0
    private var ttsHealthStatus: TtsHealthStatus = TtsHealthStatus.HEALTHY
    private var playbackController: MatchPlaybackController? = null
    private var narrationScheduler: NarrationScheduler? = null
    private var checkpointStore: MatchCheckpointStore? = null

    private lateinit var header: TextView
    private lateinit var state: TextView
    private lateinit var list: ListView
    private lateinit var playButton: Button
    private lateinit var speedButton: Button
    private lateinit var continuePeriodButton: Button
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
        engineVersion = LabEngineVersion.V4
        includeTurningPointsInShort = savedSim.includeTurningPointsInShort
        knockout = intent.getBooleanExtra(EXTRA_KNOCKOUT, false)
        seed = intent.getLongExtra(EXTRA_SEED, 123456789L)
        v4MatchId = "V4-$seed"
        if (intent.getStringExtra("resume_checkpoint_match_id").isNullOrBlank() && (home.isBlank() || away.isBlank() || home == away)) {
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
            setOnClickListener { togglePlayPause() }
        }
        playbackRow.addView(playButton, LinearLayout.LayoutParams(0, dp(48), 1f))

        speedButton = Button(this).apply {
            text = "Tốc độ: 1.0x"
            isAllCaps = false
            setOnClickListener { cyclePlaybackSpeed() }
        }
        playbackRow.addView(speedButton, LinearLayout.LayoutParams(0, dp(48), 1f))
        root.addView(playbackRow)

        continuePeriodButton = Button(this).apply {
            text = "Tiếp tục hiệp 2"
            isAllCaps = false
            visibility = View.GONE
            setOnClickListener {
                visibility = View.GONE
                playButton.text = "Tạm dừng"
                paused = false
                audio?.resumeAll()
                playbackController?.continueFromPeriodBreak()
            }
        }
        root.addView(continuePeriodButton, LinearLayout.LayoutParams(-1, dp(48)))

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
            if (finished) (records.getOrNull(position)?.get("commentary") as? String)?.let { tts?.speak(it) }
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
        audioDirector = LabAudioDirector(
            audioMgr,
            neutralGround = false,
            stadiumCapacity = estimateStadiumCapacity(home)
        )
        checkpointStore = MatchCheckpointStore(this)
        // Resume on the main thread, before any new native session can be created.
        val requestedResumeId = intent.getStringExtra("resume_checkpoint_match_id")
        if (!requestedResumeId.isNullOrBlank()) {
            if (!tryResumeActiveV4Checkpoint(requestedResumeId)) {
                state.text = "Không thể khôi phục trận đã lưu. Dữ liệu cũ vẫn được giữ lại."
            }
        } else {
            val saved = checkpointStore?.listActiveCheckpoints()?.firstOrNull()
            if (saved != null) {
                AlertDialog.Builder(this)
                    .setTitle("Có trận đấu đang lưu")
                    .setMessage("Bạn muốn tiếp tục trận đã lưu hay bắt đầu trận mới? Bản lưu cũ không bị xóa.")
                    .setPositiveButton("Tiếp tục") { _, _ ->
                        if (!tryResumeActiveV4Checkpoint(saved.matchId)) state.text = "Không thể khôi phục bản lưu."
                    }
                    .setNegativeButton("Trận mới") { _, _ -> launchMatch() }
                    .setCancelable(false).show()
            } else launchMatch()
        }
    }

    private fun togglePlayPause() {
        val ctrl = playbackController ?: return
        if (ctrl.isPaused()) {
            continuePeriodButton.visibility = View.GONE
            paused = false
            playButton.text = "Tạm dừng"
            audio?.resumeAll()
            ctrl.resume()
        } else {
            paused = true
            playButton.text = "Tiếp tục"
            audio?.pauseAll()
            ctrl.pause()
        }
    }

    private fun cyclePlaybackSpeed() {
        val rates = NarrationScheduler.SUPPORTED_PLAYBACK_RATES
        desRateIndex = (desRateIndex + 1) % rates.size
        val nextRate = rates[desRateIndex]
        speedButton.text = "Tốc độ: ${formatRateLabel(nextRate)}"
        playbackController?.setPlaybackRate(nextRate)
        Toast.makeText(this, "Tốc độ DES: ${formatRateLabel(nextRate)}", Toast.LENGTH_SHORT).show()
    }

    private fun formatRateLabel(rate: Float): String {
        return if (rate == rate.toInt().toFloat()) "${rate.toInt()}.0x" else "${rate}x"
    }

    private fun openCommandDialog() {
        commandDialogActive = true
        playbackController?.pause()
    }

    private fun closeCommandDialog() {
        commandDialogActive = false
        val ctrl = playbackController
        if (!paused && ctrl != null &&
            ctrl.currentHalfStage() == DesHalfStage.IN_HALF &&
            !isFinishing && !isDestroyed
        ) {
            ctrl.resume()
        }
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
                run {
                    val ctrl = playbackController
                    val live = liveRustSession
                    if (ctrl != null && live != null) {
                        val cmdJson = live.buildV4UpdateTacticsCommandJson("HOME", type, value)
                        ctrl.submitCommandJson(cmdJson) { ok, _ ->
                            if (!isDestroyed) {
                                Toast.makeText(
                                    this,
                                    if (ok) "Đã gửi chỉ đạo chiến thuật DES" else "Rust từ chối lệnh chiến thuật",
                                    Toast.LENGTH_SHORT
                                ).show()
                            }
                        }
                    } else {
                        executor.execute {
                            val ok = session.submitTeamInstruction(cueAdapter.liveGameSecond, "HOME", type, value)
                            main.post {
                                if (!isDestroyed) {
                                    Toast.makeText(
                                        this,
                                        if (ok) "Đã áp dụng chiến thuật" else "Rust từ chối lệnh chiến thuật",
                                        Toast.LENGTH_SHORT
                                    ).show()
                                }
                            }
                        }
                    }
                    closeCommandDialog()
                }
            }
            .setNegativeButton("Đóng") { _, _ -> closeCommandDialog() }
            .setOnCancelListener { closeCommandDialog() }
            .show()
    }

    private fun showSubstitutionDialog() {
        showV4BatchSubstitutionDialog()
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
                    val items = JSONArray()
                    for ((out, incoming) in chosen) {
                        items.put(JSONObject().put("playerOutId", out.id).put("playerInId", incoming.id))
                    }
                    val cmdJson = JSONObject()
                        .put("type", "SUBSTITUTE_BATCH")
                        .put("side", "HOME")
                        .put("changes", items)
                        .toString()
                    val ctrl = playbackController
                    if (ctrl != null) {
                        ctrl.submitCommandJson(cmdJson) { ok, _ ->
                            if (!isDestroyed) {
                                Toast.makeText(
                                    this,
                                    if (ok) "Đã xếp lịch thay ${chosen.size} cầu thủ (áp dụng ở bóng chết kế tiếp)"
                                    else "Rust từ chối nhóm thay người; đội hình không thay đổi",
                                    Toast.LENGTH_LONG
                                ).show()
                            }
                        }
                    }
                    closeCommandDialog()
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
        val applyRoster = Runnable {
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
        if (android.os.Looper.myLooper() == android.os.Looper.getMainLooper()) applyRoster.run() else main.post(applyRoster)
    }

    private fun launchMatch() {
        val token = sessionGate.begin(v4MatchId)
        executor.execute { startMatch(token) }
    }

    private fun startMatch(token: PresentationSessionGate.Token) {
        if (!sessionGate.isCurrent(token)) return
        try {
            val factory = LabMatchInputFactory(this)
            LabLogStore.log(
                "MATCH",
                "START [$engineVersion/$viewingMode] $home (${homeTactics.formation.label}/${homeTactics.style.label}) vs " +
                    "$away (${awayTactics.formation.label}/${awayTactics.style.label}) | knockout=$knockout seed=$seed"
            )
            if (!NativeBridge.isV4DesReady()) {
                throw IllegalStateException("Thư viện lõi Rust V4 DES (bridge v8 / protocol v5) chưa được đóng gói trong APK.")
            }
            val inputJson = factory.createRustV4InputJson(
                homeName = home,
                awayName = away,
                seed = seed,
                knockout = knockout,
                simulationMode = LabSimulationMode.FULL_EVENT,
                homeTactics = homeTactics,
                awayTactics = awayTactics
            )
            val matchId = JSONObject(inputJson).optString("matchId", token.matchId)
            val session = LabRustLiveSession.open(inputJson, engineVersion)
                ?: throw IllegalStateException("Không tạo được phiên Rust ${engineVersion.labelVi}")
            main.post {
                if (!sessionGate.isCurrent(token) || isFinishing || isDestroyed) {
                    session.close()
                    return@post
                }
                playbackController?.release()
                liveRustSession?.close()
                matchFactory = factory
                cueAdapter = MatchEventCueAdapter()
                v4InputJson = inputJson
                v4MatchId = matchId
                populateHomeRoster(inputJson)
                liveRustSession = session
                rustCommentary = LabCommentaryEngine(this, home, away, seed)
                startV4DesPlayback(session)
            }
        } catch (error: Exception) {
            LabLogStore.error("ERROR", "startMatch failed", error)
            main.post {
                if (!sessionGate.isCurrent(token) || isFinishing || isDestroyed) return@post
                finished = true
                state.text = "Không chạy được trận đấu: ${error.javaClass.simpleName}: ${error.message}"
            }
        }
    }

    private fun startV4DesPlayback(session: LabRustLiveSession, restored: PlaybackCheckpointState? = null) {
        val token = sessionGate.begin(v4MatchId)
        fun current() = sessionGate.isCurrent(token) && !isFinishing && !isDestroyed
        val scheduler = NarrationScheduler(
            context = this,
            ttsManager = tts,
            lookaheadWindowMs = NarrationScheduler.DEFAULT_LOOKAHEAD_MS,
            onCuePresented = { _, ev ->
                if (!current()) return@NarrationScheduler
                records.add(hashMapOf("time" to ev.minute, "commentary" to ev.text))
                recordAdapter.notifyDataSetChanged()
                list.setSelection(records.size - 1)
            },
            onHighSpeedWarning = { msg ->
                if (current()) {
                    Toast.makeText(this, msg, Toast.LENGTH_LONG).show()
                }
            },
            onHealthChanged = { status, detail ->
                if (!current()) return@NarrationScheduler
                ttsHealthStatus = status
                LabLogStore.log("TTS_HEALTH", "$status ($detail)")
            }
        )
        narrationScheduler = scheduler

        val controller = MatchPlaybackController(
            matchId = v4MatchId,
            seed = seed,
            session = session,
            narrationScheduler = scheduler,
            audioDirector = audioDirector,
            initialViewingMode = viewingMode,
            includeTurningPointsInShort = includeTurningPointsInShort,
            checkpointStore = checkpointStore,
            workerExecutor = executor,
            isSessionCurrent = { sessionGate.isCurrent(token) },
            listener = object : MatchPlaybackController.Listener {
                override fun onEventsProduced(
                    stepObj: JSONObject,
                    rawEvents: List<JSONObject>,
                    filteredEvents: List<JSONObject>,
                    isLookahead: Boolean
                ): List<NarrationScheduler.SimulationEvent> {
                    if (!current()) return emptyList()
                    return convertV4StepEvents(stepObj, rawEvents, filteredEvents)
                }

                override fun onClockUpdated(
                    snapshot: DesClockSnapshot,
                    playbackRate: Float,
                    paused: Boolean
                ) {
                    if (!current()) return
                    val displayMin = formatDisplayMinute(
                        period = snapshot.period,
                        gameSecond = (snapshot.simTimeMs / 1000L).toInt(),
                        periodElapsedSecond = (snapshot.periodElapsedMs / 1000L).toInt()
                    )
                    val healthBadge = when (ttsHealthStatus) {
                        TtsHealthStatus.HEALTHY -> ""
                        TtsHealthStatus.DEGRADED_BACKLOG -> " • TTS chậm"
                        TtsHealthStatus.BACKEND_ERROR -> " • Lỗi TTS (chạy chữ)"
                        TtsHealthStatus.UNAVAILABLE -> " • Tắt giọng đọc"
                    }
                    val score = playbackController?.presentedScore() ?: (0 to 0)
                    header.text = "$home ${score.first} – ${score.second} $away  ($displayMin')"
                    state.text = "Phút $displayMin' • DES 4.1 • ${viewingMode.labelVi} • ${formatRateLabel(playbackRate)}$healthBadge"
                }

                override fun onRosterUpdated(homeRoster: JSONObject) {
                    if (current()) {
                        syncV4HomeRoster(homeRoster)
                    }
                }

                override fun onPeriodBreak(stage: DesHalfStage, period: Int, homeGoals: Int, awayGoals: Int) {
                    if (!current()) return
                    paused = true
                    playButton.text = "Tiếp tục"
                    continuePeriodButton.text = if (stage == DesHalfStage.HALF_TIME_BREAK) {
                        "Tiếp tục hiệp 2"
                    } else {
                        "Tiếp tục hiệp phụ tiếp theo"
                    }
                    continuePeriodButton.visibility = View.VISIBLE
                    header.text = "$home $homeGoals – $awayGoals $away  (Nghỉ giữa hiệp)"
                    state.text = "Nghỉ giữa hiệp — bạn có thể chỉnh chiến thuật/thay người rồi bấm tiếp tục."
                }

                override fun onFastForwardSkip(fromSimTimeMs: Long, toSimTimeMs: Long) {
                    if (current()) {
                        val toMin = ((toSimTimeMs / 1000L) / 60L).coerceAtLeast(1L)
                        state.text = "Tua nhanh Highlights đến phút $toMin'..."
                    }
                }

                override fun onMatchFinished(finalResult: JSONObject) {
                    if (!current()) return
                    finished = true
                    paused = true
                    continuePeriodButton.visibility = View.GONE
                    updateStatsFromV4Json(finalResult)
                    saveV4SeedReplay(finalResult)
                    state.text = "Hết giờ. Chọn bất kỳ dòng nào trong danh sách để nghe lại."
                }

                override fun onError(error: Throwable) {
                    LabLogStore.error("PLAYBACK", "DES playback error", error)
                    if (current()) {
                        finished = true
                        state.text = "Lỗi phiên Rust DES: ${error.message}"
                    }
                }
            }
        )
        playbackController = controller
        if (restored == null) {
            controller.start()
        } else {
            controller.restoreFromCheckpoint(restored)
            paused = true
            playButton.text = "Tiếp tục"
        }
    }

    private fun convertV4StepEvents(
        stepObj: JSONObject,
        rawEvents: List<JSONObject>,
        filteredEvents: List<JSONObject>
    ): List<NarrationScheduler.SimulationEvent> {
        val factory = matchFactory ?: return emptyList()
        val engine = rustCommentary ?: return emptyList()
        return cueAdapter.convertV4StepEvents(stepObj, rawEvents, filteredEvents,
            viewingMode, factory::name, engine::renderRustEvent)
    }

    private fun formatDisplayMinute(period: Int, gameSecond: Int, periodElapsedSecond: Int? = null): String =
        cueAdapter.formatDisplayMinute(period, gameSecond, periodElapsedSecond)

    private fun saveV4SeedReplay(finalObj: JSONObject) {
        if (!finalObj.optBoolean("ok", false) || v4InputJson.isBlank()) return
        try {
            val matchId = finalObj.optString("matchId", v4MatchId)
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

    override fun onPause() {
        super.onPause()
        if (!finished) {
            playbackController?.onHostPaused()
            paused = true
            playButton.text = "Tiếp tục"
            audio?.pauseAll()
        }
    }

    override fun onStop() {
        super.onStop()
        if (!finished) {
            playbackController?.onHostPaused()
        }
    }

    override fun onDestroy() {
        sessionGate.invalidate()
        commandDialogActive = false
        playbackController?.release()
        narrationScheduler?.release()
        executor.shutdownNow()
        liveRustSession?.close()
        tts?.release()
        audio?.release()
        super.onDestroy()
    }

    /**
     * CRITICAL-04 / QA-08 / QA-09: End-to-end checkpoint discovery & resume flow.
     * Checks `MatchCheckpointStore` for a verified active V4 DES checkpoint, imports the Rust
     * session via `LabRustLiveSession.importFromCheckpoint`, and restores `MatchPlaybackController`
     * (including buffered unpresented events, locked 1.0x cues, and audio director state) paused at
     * the exact saved `playbackSimTimeMs`.
     */
    private fun tryResumeActiveV4Checkpoint(matchIdToResume: String? = null): Boolean {
        if (!NativeBridge.isV4DesReady()) return false
        val store = checkpointStore ?: MatchCheckpointStore(this).also { checkpointStore = it }
        val targetId = matchIdToResume ?: store.listActiveCheckpoints().firstOrNull()?.matchId ?: return false
        val envelope = store.loadVerified(targetId) ?: return false
        val restoredSession = LabRustLiveSession.importFromCheckpoint(envelope.rustCheckpointJson) ?: return false
        sessionGate.invalidate()
        playbackController?.release()
        liveRustSession?.close()
        liveRustSession = restoredSession
        v4MatchId = envelope.matchId
        val payload = JSONObject(envelope.rustCheckpointJson).getJSONObject("payload")
        val input = payload.getJSONObject("input")
        v4InputJson = input.toString()
        seed = envelope.seed
        home = input.getJSONObject("homeTeam").optString("displayName", home)
        away = input.getJSONObject("awayTeam").optString("displayName", away)
        val factory = LabMatchInputFactory(this)
        factory.registerPlayerNames(v4InputJson)
        matchFactory = factory
        populateHomeRoster(v4InputJson)
        rustCommentary = LabCommentaryEngine(this, home, away, seed)
        val rustState = payload.getJSONObject("state")
        cueAdapter = MatchEventCueAdapter().also { it.initializeScore(
            rustState.getJSONObject("home").optInt("goals", 0),
            rustState.getJSONObject("away").optInt("goals", 0)) }
        viewingMode = runCatching { LabViewingMode.valueOf(envelope.androidPlayback.viewingMode) }
            .getOrDefault(LabViewingMode.FULL_MATCH)
        includeTurningPointsInShort = envelope.androidPlayback.includeTurningPointsInShort
        startV4DesPlayback(restoredSession, envelope.androidPlayback)
        LabLogStore.log(
            "CHECKPOINT",
            "Resumed active V4 checkpoint matchId=${envelope.matchId} simTimeMs=${envelope.androidPlayback.playbackSimTimeMs}"
        )
        return true
    }
}
