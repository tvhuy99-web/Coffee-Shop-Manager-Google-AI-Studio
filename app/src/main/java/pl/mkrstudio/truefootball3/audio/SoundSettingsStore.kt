package pl.mkrstudio.truefootball3.audio

import pl.mkrstudio.truefootball3.match_audio.SoundCatalog

import android.content.Context
import android.content.SharedPreferences
import android.net.Uri
import android.provider.OpenableColumns
import java.io.File
import java.io.FileOutputStream
import java.io.IOException
import java.io.InputStream
import java.io.OutputStream

class SoundSettingsStore(context: Context) {
    class Source internal constructor(
        @JvmField val assetPath: String?,
        @JvmField val file: File?,
        @JvmField val displayName: String
    ) {
        val isAsset: Boolean get() = assetPath != null
    }

    private val appContext: Context = context.applicationContext
    private val prefs: SharedPreferences = appContext.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
    private val dir: File = File(appContext.filesDir, "custom_match_audio")

    var volumePercent: Int
        get() = prefs.getInt(VOLUME, 100).coerceIn(0, 100)
        set(percent) {
            prefs.edit().putInt(VOLUME, percent.coerceIn(0, 100)).apply()
        }

    fun resolve(slot: SoundCatalog.Slot?): Source? {
        if (slot == null || isDisabled(slot.id)) return null
        val custom = customFile(slot.id)
        if (custom.exists() && custom.isFile) {
            return Source(null, custom, prefs.getString(NAME + slot.id, custom.name) ?: custom.name)
        }
        if (slot.defaultAsset != null) {
            return Source(slot.defaultAsset, null, "Âm thanh mặc định")
        }
        return null
    }

    fun isDisabled(slotId: String): Boolean = prefs.getBoolean(DISABLED + slotId, false)

    fun hasCustom(slotId: String): Boolean = customFile(slotId).isFile

    fun stateText(slot: SoundCatalog.Slot): String = when {
        isDisabled(slot.id) -> "Đã tắt"
        hasCustom(slot.id) -> "Tùy chỉnh: ${prefs.getString(NAME + slot.id, "tệp âm thanh")}"
        slot.defaultAsset != null -> "Mặc định"
        else -> "Chưa có âm thanh"
    }

    fun setDisabled(slotId: String, disabled: Boolean) {
        prefs.edit().putBoolean(DISABLED + slotId, disabled).apply()
    }

    fun deleteCustom(slotId: String) {
        val custom = customFile(slotId)
        if (custom.exists()) custom.delete()
        prefs.edit().remove(NAME + slotId).remove(DISABLED + slotId).apply()
    }

    fun resetAll() {
        if (dir.isDirectory) {
            dir.listFiles()?.forEach { file ->
                if (file.isFile) file.delete()
            }
        }
        prefs.edit().clear().apply()
    }

    @Throws(IOException::class)
    fun installFromUri(slotId: String, uri: Uri) {
        if (SoundCatalog.find(slotId) == null) {
            throw IOException("Vị trí âm thanh không hợp lệ")
        }
        if (!dir.exists() && !dir.mkdirs()) {
            throw IOException("Không thể tạo thư mục âm thanh")
        }
        val temp = File(dir, "$slotId.incoming")
        val input = appContext.contentResolver.openInputStream(uri)
            ?: throw IOException("Không thể đọc tệp đã chọn")
        try {
            copyLimited(input, FileOutputStream(temp), MAX_AUDIO_BYTES)
        } catch (error: IOException) {
            temp.delete()
            throw error
        } finally {
            input.close()
        }
        val target = customFile(slotId)
        if (target.exists() && !target.delete()) {
            temp.delete()
            throw IOException("Không thể thay âm thanh cũ")
        }
        if (!temp.renameTo(target)) {
            temp.delete()
            throw IOException("Không thể lưu âm thanh")
        }
        prefs.edit().putString(NAME + slotId, queryName(uri)).remove(DISABLED + slotId).apply()
    }

    internal fun customFile(slotId: String): File = File(dir, "$slotId.audio")

    internal fun directory(): File = dir

    internal fun preferences(): SharedPreferences = prefs

    private fun queryName(uri: Uri): String {
        try {
            appContext.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
                if (cursor.moveToFirst()) {
                    val name = cursor.getString(0)
                    if (!name.isNullOrEmpty()) return name
                }
            }
        } catch (_: RuntimeException) {
        }
        return "Âm thanh tùy chỉnh"
    }

    companion object {
        private const val PREFS = "tf3SoundSettings"
        private const val DISABLED = "disabled_"
        private const val NAME = "name_"
        private const val VOLUME = "master_volume_percent"
        private const val MAX_AUDIO_BYTES = 100L * 1024L * 1024L

        @Throws(IOException::class)
        internal fun copyLimited(input: InputStream, output: OutputStream, limit: Long) {
            val buffer = ByteArray(16384)
            var total = 0L
            output.use { out ->
                while (true) {
                    val read = input.read(buffer)
                    if (read == -1) break
                    total += read
                    if (total > limit) throw IOException("Tệp vượt quá 100 MB")
                    out.write(buffer, 0, read)
                }
                out.flush()
            }
        }
    }
}
