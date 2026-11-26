package pl.mkrstudio.truefootball3.activities

import android.app.Activity
import android.app.AlertDialog
import android.content.Intent
import android.graphics.Color
import android.net.Uri
import android.os.Bundle
import android.view.Gravity
import android.view.View
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.SeekBar
import android.widget.TextView
import android.widget.Toast
import java.io.IOException
import pl.mkrstudio.truefootball3.R
import pl.mkrstudio.truefootball3.match_audio.SoundCatalog
import pl.mkrstudio.truefootball3.match_audio.SoundPackManager
import pl.mkrstudio.truefootball3.audio.SoundSettingsStore

class SoundSettingsActivity : Activity() {
    private lateinit var list: LinearLayout
    private lateinit var store: SoundSettingsStore
    private lateinit var volumeLabel: TextView
    private lateinit var volumeSeekBar: SeekBar

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        requestWindowFeature(1)
        store = SoundSettingsStore(this)
        buildUi()
    }

    private fun buildUi() {
        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(10), dp(12), dp(10), dp(12))
            setBackgroundResource(R.drawable.main_background)
        }

        val title = text("Cài đặt âm thanh", 23, Color.WHITE).apply {
            gravity = Gravity.CENTER
        }
        root.addView(title, fullWrap())

        volumeLabel = text("", 17, Color.WHITE).apply {
            setPadding(0, dp(10), 0, 0)
        }
        volumeSeekBar = SeekBar(this).apply {
            id = View.generateViewId()
            max = 100
            minimumHeight = dp(52)
        }
        volumeLabel.labelFor = volumeSeekBar.id
        root.addView(volumeLabel, fullWrap())
        root.addView(volumeSeekBar, fullWrap())
        refreshVolume()
        volumeSeekBar.setOnSeekBarChangeListener(object : SeekBar.OnSeekBarChangeListener {
            override fun onStartTrackingTouch(seekBar: SeekBar?) {}
            override fun onStopTrackingTouch(seekBar: SeekBar?) {}
            override fun onProgressChanged(seekBar: SeekBar?, progress: Int, fromUser: Boolean) {
                updateVolumeLabel(progress)
                if (fromUser) store.volumePercent = progress
            }
        })

        val scroll = ScrollView(this)
        list = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
        }
        scroll.addView(list, fullWrap())
        root.addView(scroll, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, 0, 1f))

        val actions = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            addView(button("Xuất bộ") { exportPack() }, weighted())
            addView(button("Nhập bộ") { importPack() }, weighted())
            addView(button("Mặc định") { confirmReset() }, weighted())
        }
        root.addView(actions, fullWrap())
        setContentView(root)
        refreshList()
    }

    private fun refreshList() {
        list.removeAllViews()
        for (group in SoundCatalog.groups()) {
            val heading = text(group.name, 17, Color.WHITE).apply {
                setBackgroundColor(0xaa173b58.toInt())
                setPadding(dp(10), dp(10), dp(10), dp(8))
            }
            val headingParams = fullWrap().apply { setMargins(0, dp(6), 0, 0) }
            list.addView(heading, headingParams)
            for (slot in group.slots) {
                val row = button("${slot.label}\n${store.stateText(slot)}") {
                    val intent = Intent(this, SoundSlotActivity::class.java)
                    intent.putExtra(SoundSlotActivity.EXTRA_SLOT_ID, slot.id)
                    startActivity(intent)
                }.apply {
                    gravity = Gravity.START or Gravity.CENTER_VERTICAL
                    isAllCaps = false
                }
                list.addView(row, fullWrap())
            }
        }
    }

    override fun onResume() {
        super.onResume()
        if (::list.isInitialized) {
            refreshList()
            refreshVolume()
        }
    }

    private fun refreshVolume() {
        if (!::volumeSeekBar.isInitialized) return
        val volume = store.volumePercent
        volumeSeekBar.progress = volume
        updateVolumeLabel(volume)
    }

    private fun updateVolumeLabel(volume: Int) {
        if (!::volumeLabel.isInitialized || !::volumeSeekBar.isInitialized) return
        volumeLabel.text = "Âm lượng âm thanh trận đấu: $volume%"
        volumeSeekBar.contentDescription = "Âm lượng âm thanh trận đấu $volume phần trăm"
    }

    @Suppress("DEPRECATION")
    private fun exportPack() {
        val intent = Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = "application/zip"
            putExtra(Intent.EXTRA_TITLE, "PingFootball_AudioPack.zip")
        }
        startActivityForResult(intent, EXPORT_PACK)
    }

    @Suppress("DEPRECATION")
    private fun importPack() {
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = "application/zip"
        }
        startActivityForResult(intent, IMPORT_PACK)
    }

    @Deprecated("Deprecated in Java")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (resultCode != RESULT_OK || data?.data == null) return
        val uri = data.data ?: return
        when (requestCode) {
            EXPORT_PACK -> runPackTask(uri, export = true)
            IMPORT_PACK -> AlertDialog.Builder(this)
                .setTitle("Nhập cả bộ âm thanh?")
                .setMessage("Bộ hiện tại sẽ được thay bằng nội dung trong tệp ZIP.")
                .setPositiveButton("Nhập") { _, _ -> runPackTask(uri, export = false) }
                .setNegativeButton("Hủy", null)
                .show()
        }
    }

    private fun runPackTask(uri: Uri, export: Boolean) {
        Toast.makeText(
            this,
            if (export) "Đang xuất bộ âm thanh…" else "Đang nhập bộ âm thanh…",
            Toast.LENGTH_SHORT
        ).show()
        Thread {
            val result = try {
                val manager = SoundPackManager(this)
                if (export) {
                    manager.exportTo(uri)
                    "Đã xuất bộ âm thanh"
                } else {
                    manager.importFrom(uri)
                    "Đã nhập bộ âm thanh"
                }
            } catch (error: IOException) {
                "Không thành công: ${error.message}"
            }
            runOnUiThread {
                Toast.makeText(this, result, Toast.LENGTH_LONG).show()
                refreshList()
            }
        }.start()
    }

    private fun confirmReset() {
        AlertDialog.Builder(this)
            .setTitle("Khôi phục âm thanh mặc định?")
            .setMessage("Mọi bản tùy chỉnh và trạng thái tắt sẽ bị xóa khỏi trò chơi. Tệp gốc trên thiết bị không bị ảnh hưởng.")
            .setPositiveButton("Khôi phục") { _, _ ->
                store.resetAll()
                refreshVolume()
                refreshList()
            }
            .setNegativeButton("Hủy", null)
            .show()
    }

    private fun text(value: String, sp: Int, color: Int): TextView = TextView(this).apply {
        text = value
        textSize = sp.toFloat()
        setTextColor(color)
    }

    private fun button(value: String, onClick: () -> Unit): Button = Button(this).apply {
        text = value
        setOnClickListener { onClick() }
    }

    private fun fullWrap(): LinearLayout.LayoutParams =
        LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT)

    private fun weighted(): LinearLayout.LayoutParams =
        LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)

    private fun dp(value: Int): Int = (value * resources.displayMetrics.density + 0.5f).toInt()

    companion object {
        private const val EXPORT_PACK = 7101
        private const val IMPORT_PACK = 7102
    }
}
