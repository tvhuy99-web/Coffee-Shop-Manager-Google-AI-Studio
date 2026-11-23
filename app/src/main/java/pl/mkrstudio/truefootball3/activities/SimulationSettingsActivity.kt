package pl.mkrstudio.truefootball3.activities

import android.app.Activity
import android.graphics.Color
import android.os.Build
import android.os.Bundle
import android.view.View
import android.widget.Button
import android.widget.CheckBox
import android.widget.LinearLayout
import android.widget.RadioButton
import android.widget.RadioGroup
import android.widget.ScrollView
import android.widget.TextView
import android.widget.Toast
import pl.mkrstudio.truefootball3.audio.TtsSettingsStore
import pl.mkrstudio.truefootball3.lab.LabEngineVersion
import pl.mkrstudio.truefootball3.lab.LabViewingMode
import pl.mkrstudio.truefootball3.lab.SimulationSettingsStore
import java.util.Locale

/**
 * Independent "Cài đặt mô phỏng" screen alongside "Cài đặt TTS" and "Cài đặt âm thanh".
 * Synchronized bidirectionally with the Trial screen for viewing mode (Full trận / Chi tiết / Ngắn gọn)
 * and simulation engine mode (3.0 / 4.0).
 */
class SimulationSettingsActivity : Activity() {
    private lateinit var store: SimulationSettingsStore
    private fun dp(v: Int) = (v * resources.displayMetrics.density).toInt()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        store = SimulationSettingsStore(this)
        val current = store.load()
        val ttsSelection = TtsSettingsStore(this).load()
        val ttsRate = (ttsSelection.ratePercent / 100f).coerceIn(0.5f, 5.0f)

        val column = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(18), dp(16), dp(18), dp(22))
            setBackgroundColor(Color.rgb(16, 28, 41))
        }

        column.addView(TextView(this).apply {
            text = "CÀI ĐẶT MÔ PHỎNG"
            textSize = 23f
            setTextColor(Color.WHITE)
            setPadding(0, dp(4), 0, dp(14))
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
                isAccessibilityHeading = true
            }
        })

        column.addView(TextView(this).apply {
            text = "Các lựa chọn dưới đây được đồng bộ trực tiếp với màn hình Thử nghiệm trận đấu."
            textSize = 15f
            setTextColor(Color.rgb(210, 226, 240))
            setPadding(0, 0, 0, dp(12))
        })

        // --- Engine version selector (3.0 / 4.0) ---
        column.addView(TextView(this).apply {
            text = "Chế độ mô phỏng"
            textSize = 17f
            setTextColor(Color.rgb(185, 224, 255))
            setPadding(0, dp(4), 0, dp(4))
        })

        val engineGroup = RadioGroup(this).apply { orientation = RadioGroup.VERTICAL }
        val rbV3 = RadioButton(this).apply {
            id = View.generateViewId()
            text = "Phiên bản 3.0 (20 chỉ số — bản tham chiếu)"
            textSize = 16f
            setTextColor(Color.WHITE)
            minHeight = dp(48)
        }
        val rbV4 = RadioButton(this).apply {
            id = View.generateViewId()
            text = "Phiên bản 4.0 (54 chỉ số — 90 phút)"
            textSize = 16f
            setTextColor(Color.WHITE)
            minHeight = dp(48)
        }
        engineGroup.addView(rbV3)
        engineGroup.addView(rbV4)
        when (current.engineVersion) {
            LabEngineVersion.V3 -> engineGroup.check(rbV3.id)
            LabEngineVersion.V4 -> engineGroup.check(rbV4.id)
        }
        column.addView(engineGroup, LinearLayout.LayoutParams(-1, -2).apply { bottomMargin = dp(12) })

        // --- Viewing mode selector (Full trận / Chi tiết / Ngắn gọn) ---
        column.addView(TextView(this).apply {
            text = "Chế độ xem trận đấu"
            textSize = 17f
            setTextColor(Color.rgb(185, 224, 255))
            setPadding(0, dp(4), 0, dp(4))
        })

        val modeGroup = RadioGroup(this).apply { orientation = RadioGroup.VERTICAL }

        val rbFull = RadioButton(this).apply {
            id = View.generateViewId()
            text = "Full trận — phát toàn bộ diễn biến"
            textSize = 16f
            setTextColor(Color.WHITE)
            minHeight = dp(48)
        }
        val rbExtended = RadioButton(this).apply {
            id = View.generateViewId()
            text = "Chi tiết — cơ hội nguy hiểm, cứu thua, cố định, thẻ phạt, bàn thắng"
            textSize = 16f
            setTextColor(Color.WHITE)
            minHeight = dp(48)
        }
        val rbShort = RadioButton(this).apply {
            id = View.generateViewId()
            text = "Ngắn gọn — chỉ bàn thắng (và bước ngoặt quan trọng)"
            textSize = 16f
            setTextColor(Color.WHITE)
            minHeight = dp(48)
        }

        modeGroup.addView(rbFull)
        modeGroup.addView(rbExtended)
        modeGroup.addView(rbShort)

        when (current.viewingMode) {
            LabViewingMode.FULL_MATCH -> modeGroup.check(rbFull.id)
            LabViewingMode.EXTENDED_HIGHLIGHTS -> modeGroup.check(rbExtended.id)
            LabViewingMode.SHORT_HIGHLIGHTS -> modeGroup.check(rbShort.id)
        }
        column.addView(modeGroup, LinearLayout.LayoutParams(-1, -2).apply { bottomMargin = dp(12) })

        val cbTurningPoints = CheckBox(this).apply {
            text = "Trong chế độ Ngắn gọn, hiển thị kèm phạt đền và thẻ đỏ"
            textSize = 15f
            setTextColor(Color.WHITE)
            minHeight = dp(48)
            isChecked = current.includeTurningPointsInShort
        }
        column.addView(cbTurningPoints, LinearLayout.LayoutParams(-1, -2).apply { bottomMargin = dp(8) })

        val cbSyncTts = CheckBox(this).apply {
            text = String.format(
                Locale.US,
                "Đồng bộ tốc độ trận đấu theo tốc độ đọc TTS (hiện tại: %.2fx)",
                ttsRate
            )
            textSize = 15f
            setTextColor(Color.WHITE)
            minHeight = dp(48)
            isChecked = current.syncMatchSpeedWithTts
        }
        column.addView(cbSyncTts, LinearLayout.LayoutParams(-1, -2).apply { bottomMargin = dp(16) })

        column.addView(
            Button(this).apply {
                text = "Lưu cài đặt mô phỏng"
                isAllCaps = false
                minHeight = dp(54)
                textSize = 18f
                setOnClickListener {
                    val selectedMode = when (modeGroup.checkedRadioButtonId) {
                        rbExtended.id -> LabViewingMode.EXTENDED_HIGHLIGHTS
                        rbShort.id -> LabViewingMode.SHORT_HIGHLIGHTS
                        else -> LabViewingMode.FULL_MATCH
                    }
                    val selectedEngine = when (engineGroup.checkedRadioButtonId) {
                        rbV3.id -> LabEngineVersion.V3
                        else -> LabEngineVersion.V4
                    }
                    store.save(
                        SimulationSettingsStore.Settings(
                            viewingMode = selectedMode,
                            engineVersion = selectedEngine,
                            includeTurningPointsInShort = cbTurningPoints.isChecked,
                            syncMatchSpeedWithTts = cbSyncTts.isChecked
                        )
                    )
                    Toast.makeText(
                        this@SimulationSettingsActivity,
                        "Đã lưu: ${selectedEngine.labelVi} • ${selectedMode.labelVi}",
                        Toast.LENGTH_SHORT
                    ).show()
                    finish()
                }
            },
            LinearLayout.LayoutParams(-1, -2).apply { bottomMargin = dp(10) }
        )

        column.addView(
            Button(this).apply {
                text = "Quay lại"
                isAllCaps = false
                minHeight = dp(54)
                textSize = 18f
                setOnClickListener { finish() }
            },
            LinearLayout.LayoutParams(-1, -2)
        )

        val scroll = ScrollView(this).apply {
            isFillViewport = true
            addView(column)
        }
        setContentView(scroll)
    }
}
