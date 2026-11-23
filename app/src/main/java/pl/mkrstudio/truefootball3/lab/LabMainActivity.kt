package pl.mkrstudio.truefootball3.lab

import android.app.Activity
import android.app.AlertDialog
import android.content.Intent
import android.graphics.Color
import android.graphics.Typeface
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.view.View
import android.widget.ArrayAdapter
import android.widget.AdapterView
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.Spinner
import android.widget.TextView
import android.widget.Toast
import org.json.JSONObject
import java.util.concurrent.Executors
import pl.mkrstudio.truefootball3.activities.SoundSettingsActivity
import pl.mkrstudio.truefootball3.activities.TtsSettingsActivity

class LabMainActivity : Activity() {
    private enum class Screen { MAIN, SETTINGS, TRIAL, LOGS }

    private data class ClubChoice(val name: String, val leagueId: String)
    private var clubChoices: List<ClubChoice> = emptyList()
    private var teamNames: List<String> = emptyList()
    private var currentScreen: Screen = Screen.MAIN
    private val batchExecutor = Executors.newSingleThreadExecutor()
    private val mainHandler = Handler(Looper.getMainLooper())
    private fun dp(v: Int) = (v * resources.displayMetrics.density).toInt()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        LabLogStore.init(this)
        teamNames = try {
            val text = assets.open("england_v2/clubs.json").bufferedReader().use { it.readText() }
            val clubs = JSONObject(text).getJSONArray("clubs")
            clubChoices = (0 until clubs.length()).map {
                val row = clubs.getJSONObject(it)
                ClubChoice(row.getString("name"), row.getString("league_id"))
            }
            clubChoices.map { it.name }.sorted()
        } catch (_: Exception) {
            emptyList()
        }
        showMain()
    }

    private fun vertical(title: String): LinearLayout {
        val column = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(18), dp(16), dp(18), dp(22))
            setBackgroundColor(Color.rgb(16, 28, 41))
        }
        val heading = TextView(this).apply {
            text = title
            textSize = 23f
            setTextColor(Color.WHITE)
            setPadding(0, dp(4), 0, dp(18))
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
                isAccessibilityHeading = true
            }
        }
        column.addView(heading)
        val scroll = ScrollView(this).apply { isFillViewport = true; addView(column) }
        setContentView(scroll)
        return column
    }

    private fun button(host: LinearLayout, name: String, onClick: () -> Unit) {
        host.addView(
            Button(this).apply {
                text = name
                isAllCaps = false
                minHeight = dp(54)
                textSize = 18f
                setOnClickListener { onClick() }
            },
            LinearLayout.LayoutParams(-1, -2).apply { bottomMargin = dp(10) }
        )
    }

    private fun info(host: LinearLayout, message: String) {
        host.addView(
            TextView(this).apply {
                text = message
                textSize = 15f
                setTextColor(Color.rgb(219, 232, 242))
                setPadding(0, dp(5), 0, dp(12))
            }
        )
    }

    private fun showMain() {
        currentScreen = Screen.MAIN
        val root = vertical("FOOTBALL MATCH LAB")
        button(root, "Bắt đầu") { Toast.makeText(this, "Chưa triển khai", Toast.LENGTH_SHORT).show() }
        button(root, "Tải lại") { Toast.makeText(this, "Chưa triển khai", Toast.LENGTH_SHORT).show() }
        button(root, "Cài đặt") { showSettingsMenu() }
        val trialLogRow = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
        trialLogRow.addView(
            Button(this).apply {
                text = "Thử nghiệm"
                isAllCaps = false
                minHeight = dp(54)
                textSize = 18f
                setOnClickListener {
                    LabLogStore.log("UI", "Mở màn hình Thử nghiệm trận đấu")
                    showTrial()
                }
            },
            LinearLayout.LayoutParams(0, -2, 1f).apply { rightMargin = dp(6); bottomMargin = dp(10) }
        )
        trialLogRow.addView(
            Button(this).apply {
                text = "Nhật ký"
                isAllCaps = false
                minHeight = dp(54)
                textSize = 18f
                setOnClickListener { showLogs() }
            },
            LinearLayout.LayoutParams(0, -2, 1f).apply { leftMargin = dp(6); bottomMargin = dp(10) }
        )
        root.addView(trialLogRow, LinearLayout.LayoutParams(-1, -2))
        info(root, "${NativeBridge.status()}. Kiến trúc chuẩn: lõi mô phỏng Rust + tầng Android Kotlin.")
    }

    private fun showLogs() {
        currentScreen = Screen.LOGS
        val root = vertical("NHẬT KÝ HOẠT ĐỘNG")
        val statusView = TextView(this).apply {
            textSize = 14f
            setTextColor(Color.rgb(185, 224, 255))
            setPadding(0, 0, 0, dp(8))
        }
        root.addView(statusView)

        val logView = TextView(this).apply {
            textSize = 12f
            typeface = Typeface.MONOSPACE
            setTextColor(Color.rgb(230, 240, 248))
            setBackgroundColor(Color.rgb(10, 18, 28))
            setPadding(dp(10), dp(10), dp(10), dp(10))
            setTextIsSelectable(true)
        }

        lateinit var toggleBtn: Button
        fun refreshLogs() {
            val on = LabLogStore.isEnabled()
            val len = LabLogStore.length()
            statusView.text = "Trạng thái: ${if (on) "ĐANG BẬT" else "ĐANG TẮT"} | $len / ${LabLogStore.MAX_CHARS} ký tự"
            toggleBtn.text = if (on) "Tắt nhật ký" else "Bật nhật ký"
            val preview = LabLogStore.getPreviewText()
            logView.text = if (preview.isEmpty()) "(Chưa có nhật ký nào được ghi)" else preview
        }

        val actions = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
        toggleBtn = Button(this).apply {
            isAllCaps = false
            setOnClickListener {
                LabLogStore.setEnabled(!LabLogStore.isEnabled())
                refreshLogs()
            }
        }
        actions.addView(toggleBtn, LinearLayout.LayoutParams(0, dp(48), 1f))
        actions.addView(
            Button(this).apply {
                text = "Sao chép"
                isAllCaps = false
                setOnClickListener {
                    val copied = LabLogStore.copyToClipboard(this@LabMainActivity)
                    Toast.makeText(
                        this@LabMainActivity,
                        if (copied > 0) "Đã sao chép $copied ký tự vào bộ nhớ tạm" else "Nhật ký đang trống",
                        Toast.LENGTH_SHORT
                    ).show()
                }
            },
            LinearLayout.LayoutParams(0, dp(48), 1f)
        )
        actions.addView(
            Button(this).apply {
                text = "Xóa"
                isAllCaps = false
                setOnClickListener {
                    LabLogStore.clear()
                    refreshLogs()
                    Toast.makeText(this@LabMainActivity, "Đã xóa toàn bộ nhật ký", Toast.LENGTH_SHORT).show()
                }
            },
            LinearLayout.LayoutParams(0, dp(48), 1f)
        )
        root.addView(actions, LinearLayout.LayoutParams(-1, -2).apply { bottomMargin = dp(10) })
        root.addView(logView, LinearLayout.LayoutParams(-1, -2).apply { bottomMargin = dp(12) })
        button(root, "Làm mới") { refreshLogs() }
        button(root, "Quay lại") { showMain() }
        refreshLogs()
    }

    private fun showSettingsMenu() {
        currentScreen = Screen.SETTINGS
        val root = vertical("CÀI ĐẶT")
        button(root, "Cài đặt TTS") { startActivity(Intent(this, TtsSettingsActivity::class.java)) }
        button(root, "Cài đặt âm thanh") { startActivity(Intent(this, SoundSettingsActivity::class.java)) }
        button(root, "Quay lại") { showMain() }
    }

    private fun showTrial() {
        currentScreen = Screen.TRIAL
        val root = vertical("THỬ NGHIỆM TRẬN ĐẤU")
        info(root, "Chọn giải đấu Anh, hai đội bóng, sơ đồ và chiến thuật. Chỉ số 54 thuộc tính được chiếu tạm sang lõi trận 20 chỉ số Rust; chưa phải tích hợp native 54 thuộc tính.")
        if (teamNames.size < 20 || clubChoices.none { it.leagueId == "ENG_1" }) {
            info(root, "Lỗi dữ liệu giai đoạn 2: không tìm thấy đủ câu lạc bộ và giải đấu.")
            button(root, "Quay lại") { showMain() }
            return
        }

        fun makeSpinner(label: String, items: List<String>, defaultIndex: Int = 0): Spinner {
            val adapter = ArrayAdapter(this, android.R.layout.simple_spinner_item, items).apply {
                setDropDownViewResource(android.R.layout.simple_spinner_dropdown_item)
            }
            root.addView(TextView(this).apply {
                text = label
                textSize = 16f
                setTextColor(Color.WHITE)
                labelFor = View.NO_ID
            })
            return Spinner(this).apply {
                id = View.generateViewId()
                setAdapter(adapter)
                setSelection(defaultIndex.coerceIn(0, items.lastIndex))
                contentDescription = label
                root.addView(this, LinearLayout.LayoutParams(-1, dp(48)))
            }
        }

        val formLabels = LabTacticalPresets.FORMATIONS.map { it.label }
        val styleLabels = LabTacticalPresets.STYLES.map { it.label }

        val leagues = listOf(
            "ENG_1" to "Premier League",
            "ENG_2" to "EFL Championship",
            "ENG_3" to "EFL League One",
            "ENG_4" to "EFL League Two",
            "ENG_5" to "National League",
            "ENG_6A" to "National League North",
            "ENG_6B" to "National League South",
            "ENG_7_1" to "Hạng 7 nhóm thử nghiệm 1",
            "ENG_7_2" to "Hạng 7 nhóm thử nghiệm 2",
            "ENG_7_3" to "Hạng 7 nhóm thử nghiệm 3",
            "ENG_7_4" to "Hạng 7 nhóm thử nghiệm 4"
        )
        val leagueSpinner = makeSpinner("Giải đấu", leagues.map { it.second }, 0)
        val initialTeams = clubChoices.filter { it.leagueId == "ENG_1" }.map { it.name }.sorted()
        val home = makeSpinner("Đội chủ nhà", initialTeams, 0)
        val homeForm = makeSpinner("Sơ đồ chủ nhà", formLabels, 0)
        val homeStyle = makeSpinner("Chiến thuật chủ nhà", styleLabels, 0)

        val away = makeSpinner("Đội khách", initialTeams, 1)
        leagueSpinner.onItemSelectedListener = object : AdapterView.OnItemSelectedListener {
            override fun onNothingSelected(parent: AdapterView<*>?) = Unit
            override fun onItemSelected(parent: AdapterView<*>?, view: View?, position: Int, id: Long) {
                val selectedLeague = leagues[position].first
                val clubs = clubChoices.filter { it.leagueId == selectedLeague }.map { it.name }.sorted()
                val homeAdapter = ArrayAdapter(this@LabMainActivity, android.R.layout.simple_spinner_item, clubs).apply {
                    setDropDownViewResource(android.R.layout.simple_spinner_dropdown_item)
                }
                val awayAdapter = ArrayAdapter(this@LabMainActivity, android.R.layout.simple_spinner_item, clubs).apply {
                    setDropDownViewResource(android.R.layout.simple_spinner_dropdown_item)
                }
                home.adapter = homeAdapter
                away.adapter = awayAdapter
                if (clubs.size > 1) away.setSelection(1)
            }
        }
        val awayForm = makeSpinner("Sơ đồ đội khách", formLabels, 0)
        val awayStyle = makeSpinner("Chiến thuật đội khách", styleLabels, 0)

        val durationSpinner = makeSpinner(
            "Thời lượng mô phỏng",
            listOf("20 phút hành động (đồng hồ 90')", "90 phút đầy đủ"),
            0
        )
        val rulesSpinner = makeSpinner(
            "Luật thi đấu",
            listOf("Giải VĐQG (90 phút)", "Loại trực tiếp (Hiệp phụ + Luân lưu)"),
            0
        )
        val seedSpinner = makeSpinner(
            "Seed ngẫu nhiên",
            listOf("Ngẫu nhiên mỗi trận", "Seed cố định (123456789)"),
            0
        )

        button(root, "Xem đội hình tối ưu & Đá cố định") {
            val a = home.selectedItem.toString()
            val b = away.selectedItem.toString()
            if (a == b) {
                Toast.makeText(this, "Vui lòng chọn hai đội khác nhau.", Toast.LENGTH_SHORT).show()
                return@button
            }
            val hTac = LabTacticalPresets.TacticalConfig(
                LabTacticalPresets.FORMATIONS[homeForm.selectedItemPosition],
                LabTacticalPresets.STYLES[homeStyle.selectedItemPosition]
            )
            val aTac = LabTacticalPresets.TacticalConfig(
                LabTacticalPresets.FORMATIONS[awayForm.selectedItemPosition],
                LabTacticalPresets.STYLES[awayStyle.selectedItemPosition]
            )
            val factory = LabMatchInputFactory(this)
            val preview = factory.describeLineupAndSetPieces(a, hTac) + "\n" + factory.describeLineupAndSetPieces(b, aTac)
            AlertDialog.Builder(this)
                .setTitle("Đội hình tối ưu & Phân công cố định")
                .setMessage(preview)
                .setPositiveButton("Đóng", null)
                .show()
        }

        button(root, "Bắt đầu trận đấu") {
            val a = home.selectedItem.toString()
            val b = away.selectedItem.toString()
            if (a == b) {
                AlertDialog.Builder(this)
                    .setMessage("Vui lòng chọn hai đội khác nhau.")
                    .setPositiveButton("Đồng ý", null)
                    .show()
            } else {
                val seed = if (seedSpinner.selectedItemPosition == 1) 123456789L else System.currentTimeMillis()
                startActivity(Intent(this, LabReferenceMatchActivity::class.java).apply {
                    putExtra(LabReferenceMatchActivity.EXTRA_HOME, a)
                    putExtra(LabReferenceMatchActivity.EXTRA_AWAY, b)
                    putExtra(LabReferenceMatchActivity.EXTRA_HOME_FORMATION, LabTacticalPresets.FORMATIONS[homeForm.selectedItemPosition].id)
                    putExtra(LabReferenceMatchActivity.EXTRA_HOME_STYLE, LabTacticalPresets.STYLES[homeStyle.selectedItemPosition].id)
                    putExtra(LabReferenceMatchActivity.EXTRA_AWAY_FORMATION, LabTacticalPresets.FORMATIONS[awayForm.selectedItemPosition].id)
                    putExtra(LabReferenceMatchActivity.EXTRA_AWAY_STYLE, LabTacticalPresets.STYLES[awayStyle.selectedItemPosition].id)
                    putExtra(LabReferenceMatchActivity.EXTRA_SIM_MINUTES, if (durationSpinner.selectedItemPosition == 0) 20 else 90)
                    putExtra(LabReferenceMatchActivity.EXTRA_KNOCKOUT, rulesSpinner.selectedItemPosition == 1)
                    putExtra(LabReferenceMatchActivity.EXTRA_SEED, seed)
                })
            }
        }

        button(root, "Chạy nhanh 20 trận (Kiểm chuẩn)") {
            val a = home.selectedItem.toString()
            val b = away.selectedItem.toString()
            if (a == b) {
                Toast.makeText(this, "Vui lòng chọn hai đội khác nhau.", Toast.LENGTH_SHORT).show()
                return@button
            }
            val simMins = if (durationSpinner.selectedItemPosition == 0) 20 else 90
            val knockout = rulesSpinner.selectedItemPosition == 1
            val hTac = LabTacticalPresets.TacticalConfig(
                LabTacticalPresets.FORMATIONS[homeForm.selectedItemPosition],
                LabTacticalPresets.STYLES[homeStyle.selectedItemPosition]
            )
            val aTac = LabTacticalPresets.TacticalConfig(
                LabTacticalPresets.FORMATIONS[awayForm.selectedItemPosition],
                LabTacticalPresets.STYLES[awayStyle.selectedItemPosition]
            )
            runBatchCalibration(a, b, 20, simMins, knockout, hTac, aTac)
        }

        button(root, "Quay lại") { showMain() }
    }

    private fun runBatchCalibration(
        home: String,
        away: String,
        matches: Int,
        simMinutes: Int,
        knockout: Boolean,
        homeTactics: LabTacticalPresets.TacticalConfig,
        awayTactics: LabTacticalPresets.TacticalConfig
    ) {
        if (!NativeBridge.isFullEngineReady()) {
            Toast.makeText(this, "Thư viện lõi Rust (.so) chưa sẵn sàng trong APK.", Toast.LENGTH_LONG).show()
            return
        }
        Toast.makeText(this, "Đang chạy $matches trận kiểm chuẩn bằng Rust...", Toast.LENGTH_SHORT).show()
        batchExecutor.execute {
            val t0 = SystemClock.elapsedRealtime()
            val factory = LabMatchInputFactory(this)
            var homeWins = 0
            var draws = 0
            var awayWins = 0
            var totalHomeGoals = 0
            var totalAwayGoals = 0
            var totalHomeShots = 0
            var totalAwayShots = 0
            var totalHomeXgMilli = 0L
            var totalAwayXgMilli = 0L
            val baseSeed = 20260000L

            try {
                for (i in 0 until matches) {
                    val s = baseSeed + i * 7919L
                    val json = factory.createRustInputJson(home, away, s, simMinutes, knockout, homeTactics, awayTactics)
                    val resp = JSONObject(NativeBridge.simulateMatch(json))
                    if (!resp.optBoolean("ok", false)) {
                        throw IllegalStateException(resp.optString("error", "Rust simulate failed"))
                    }
                    val hg = resp.optInt("homeGoals", 0)
                    val ag = resp.optInt("awayGoals", 0)
                    totalHomeGoals += hg
                    totalAwayGoals += ag
                    if (hg > ag) homeWins++ else if (ag > hg) awayWins++ else draws++
                    val stats = resp.optJSONObject("statistics")
                    val hs = stats?.optJSONObject("home")
                    val asObj = stats?.optJSONObject("away")
                    totalHomeShots += hs?.optInt("shots", 0) ?: 0
                    totalAwayShots += asObj?.optInt("shots", 0) ?: 0
                    totalHomeXgMilli += hs?.optLong("expectedGoalsMilli", 0L) ?: 0L
                    totalAwayXgMilli += asObj?.optLong("expectedGoalsMilli", 0L) ?: 0L
                }
                val elapsedMs = SystemClock.elapsedRealtime() - t0
                val summary = buildString {
                    appendLine("Động cơ: Rust Engine ($matches trận trong ${elapsedMs}ms)")
                    appendLine("$home (${homeTactics.formation.label}/${homeTactics.style.label})")
                    appendLine("vs $away (${awayTactics.formation.label}/${awayTactics.style.label})")
                    appendLine()
                    appendLine("Thắng / Hòa / Thua: $homeWins – $draws – $awayWins")
                    appendLine("Bàn trung bình/trận: ${"%.2f".format(totalHomeGoals.toDouble() / matches)} – ${"%.2f".format(totalAwayGoals.toDouble() / matches)}")
                    appendLine("xG trung bình/trận: ${"%.2f".format(totalHomeXgMilli / 1000.0 / matches)} – ${"%.2f".format(totalAwayXgMilli / 1000.0 / matches)}")
                    appendLine("Sút trung bình/trận: ${"%.1f".format(totalHomeShots.toDouble() / matches)} – ${"%.1f".format(totalAwayShots.toDouble() / matches)}")
                }
                LabLogStore.log("CALIBRATION", summary.replace("\n", " | "))
                mainHandler.post {
                    AlertDialog.Builder(this)
                        .setTitle("Kết quả kiểm chuẩn ($matches trận)")
                        .setMessage(summary)
                        .setPositiveButton("Đóng", null)
                        .show()
                }
            } catch (e: Exception) {
                LabLogStore.error("ERROR", "Batch calibration failed", e)
                mainHandler.post {
                    AlertDialog.Builder(this)
                        .setTitle("Lỗi kiểm chuẩn")
                        .setMessage("${e.javaClass.simpleName}: ${e.message}")
                        .setPositiveButton("Đóng", null)
                        .show()
                }
            }
        }
    }

    @Deprecated("Handled by in-screen navigation")
    override fun onBackPressed() {
        if (currentScreen != Screen.MAIN) {
            showMain()
        } else {
            @Suppress("DEPRECATION")
            super.onBackPressed()
        }
    }

    override fun onDestroy() {
        batchExecutor.shutdownNow()
        super.onDestroy()
    }
}
