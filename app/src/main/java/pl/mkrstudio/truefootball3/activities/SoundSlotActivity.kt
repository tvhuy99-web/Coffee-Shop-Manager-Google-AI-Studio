package pl.mkrstudio.truefootball3.activities

import android.app.Activity
import android.app.AlertDialog
import android.content.Intent
import android.content.res.AssetFileDescriptor
import android.graphics.Color
import android.media.AudioAttributes
import android.media.MediaPlayer
import android.net.Uri
import android.os.Bundle
import android.view.Gravity
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView
import android.widget.Toast
import java.io.IOException
import pl.mkrstudio.truefootball3.R
import pl.mkrstudio.truefootball3.audio.SoundCatalog
import pl.mkrstudio.truefootball3.audio.SoundSettingsStore

class SoundSlotActivity : Activity() {
    private lateinit var slot: SoundCatalog.Slot
    private lateinit var store: SoundSettingsStore
    private lateinit var status: TextView
    private lateinit var deleteButton: Button
    private lateinit var toggleButton: Button
    private var player: MediaPlayer? = null
    private var candidateUri: Uri? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        requestWindowFeature(1)
        val found = SoundCatalog.find(intent.getStringExtra(EXTRA_SLOT_ID))
        if (found == null) {
            finish()
            return
        }
        slot = found
        store = SoundSettingsStore(this)
        buildUi()
    }

    private fun buildUi() {
        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(18), dp(20), dp(18), dp(20))
            setBackgroundResource(R.drawable.main_background)
        }

        val title = text(slot.group, 22, Color.WHITE).apply { gravity = Gravity.CENTER }
        root.addView(title, fullWrap())
        val label = text(slot.label, 17, 0xffd8ebff.toInt()).apply { gravity = Gravity.CENTER }
        root.addView(label, fullWrap().apply { setMargins(0, dp(8), 0, dp(18)) })

        status = text("", 15, Color.WHITE).apply { gravity = Gravity.CENTER }
        root.addView(status, fullWrap().apply { setMargins(0, 0, 0, dp(14)) })

        root.addView(button("▶  Phát âm thanh hiện tại") { playCurrent() }, fullWrap())
        root.addView(button("Chọn âm thanh khác") { openPicker() }, fullWrap())
        deleteButton = button("Xóa âm thanh tùy chỉnh") { confirmDelete() }
        root.addView(deleteButton, fullWrap())
        toggleButton = button("") {
            store.setDisabled(slot.id, !store.isDisabled(slot.id))
            stopPlayer()
            refresh()
        }
        root.addView(toggleButton, fullWrap())
        root.addView(button("Quay lại") { finish() }, fullWrap())
        setContentView(root)
        refresh()
    }

    private fun refresh() {
        status.text = "Trạng thái: ${store.stateText(slot)}"
        deleteButton.isEnabled = store.hasCustom(slot.id)
        toggleButton.text = if (store.isDisabled(slot.id)) "Bật vị trí âm thanh này" else "Tắt vị trí âm thanh này"
    }

    private fun playCurrent() {
        val source = store.resolve(slot)
        if (source == null) {
            Toast.makeText(this, "Vị trí này hiện không có âm thanh", Toast.LENGTH_SHORT).show()
            return
        }
        playSource(source)
    }

    private fun playSource(source: SoundSettingsStore.Source) {
        stopPlayer()
        var descriptor: AssetFileDescriptor? = null
        try {
            val mp = MediaPlayer().apply {
                setAudioAttributes(
                    AudioAttributes.Builder()
                        .setUsage(AudioAttributes.USAGE_MEDIA)
                        .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
                        .build()
                )
            }
            player = mp
            if (source.isAsset) {
                descriptor = assets.openFd(source.assetPath!!)
                mp.setDataSource(descriptor.fileDescriptor, descriptor.startOffset, descriptor.length)
            } else {
                mp.setDataSource(source.file!!.absolutePath)
            }
            prepareAndStart(mp)
        } catch (_: IOException) {
            stopPlayer()
            Toast.makeText(this, "Không thể phát âm thanh", Toast.LENGTH_SHORT).show()
        } finally {
            try {
                descriptor?.close()
            } catch (_: IOException) {
            }
        }
    }

    @Suppress("DEPRECATION")
    private fun openPicker() {
        stopPlayer()
        startActivityForResult(Intent(this, AudioFilePickerActivity::class.java), PICK_AUDIO)
    }

    @Deprecated("Deprecated in Java")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != PICK_AUDIO || resultCode != RESULT_OK || data?.data == null) return
        val uri = data.data ?: return
        candidateUri = uri
        previewCandidate(uri)
        showCandidateDialog()
    }

    private fun previewCandidate(uri: Uri) {
        stopPlayer()
        try {
            val mp = MediaPlayer().apply {
                setAudioAttributes(
                    AudioAttributes.Builder()
                        .setUsage(AudioAttributes.USAGE_MEDIA)
                        .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
                        .build()
                )
                setDataSource(this@SoundSlotActivity, uri)
            }
            player = mp
            prepareAndStart(mp)
        } catch (_: IOException) {
            stopPlayer()
            Toast.makeText(this, "Tệp này không phát được", Toast.LENGTH_SHORT).show()
        }
    }

    private fun showCandidateDialog() {
        AlertDialog.Builder(this)
            .setTitle("Nghe thử âm thanh")
            .setMessage("Âm thanh đã tự động phát. Bạn muốn dùng tệp này cho “${slot.label}” không?")
            .setPositiveButton("Dùng âm thanh này") { _, _ -> installCandidate() }
            .setNeutralButton("Chọn file khác") { _, _ -> openPicker() }
            .setNegativeButton("Hủy", null)
            .show()
    }

    private fun installCandidate() {
        val uri = candidateUri ?: return
        try {
            store.installFromUri(slot.id, uri)
            Toast.makeText(this, "Đã dùng âm thanh mới", Toast.LENGTH_SHORT).show()
            refresh()
        } catch (error: IOException) {
            Toast.makeText(this, error.message, Toast.LENGTH_LONG).show()
        }
    }

    private fun confirmDelete() {
        AlertDialog.Builder(this)
            .setTitle("Xóa âm thanh tùy chỉnh?")
            .setMessage("Chỉ bản sao trong trò chơi bị xóa. Tệp gốc trên thiết bị vẫn được giữ nguyên.")
            .setPositiveButton("Xóa") { _, _ ->
                stopPlayer()
                store.deleteCustom(slot.id)
                refresh()
            }
            .setNegativeButton("Hủy", null)
            .show()
    }

    private fun prepareAndStart(mediaPlayer: MediaPlayer) {
        val volume = store.volumePercent / 100f
        mediaPlayer.setVolume(volume, volume)
        mediaPlayer.setOnPreparedListener { it.start() }
        mediaPlayer.setOnCompletionListener { stopPlayer() }
        mediaPlayer.prepareAsync()
    }

    private fun stopPlayer() {
        player?.let {
            try {
                it.reset()
            } catch (_: IllegalStateException) {
            }
            it.release()
            player = null
        }
    }

    override fun onStop() {
        stopPlayer()
        super.onStop()
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

    private fun dp(value: Int): Int = (value * resources.displayMetrics.density + 0.5f).toInt()

    companion object {
        const val EXTRA_SLOT_ID = "slot_id"
        private const val PICK_AUDIO = 7001
    }
}
