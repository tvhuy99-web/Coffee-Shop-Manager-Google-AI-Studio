package pl.mkrstudio.truefootball3.match_audio

import pl.mkrstudio.truefootball3.audio.*

import android.content.Context
import android.net.Uri
import java.io.BufferedInputStream
import java.io.BufferedOutputStream
import java.io.File
import java.io.FileInputStream
import java.io.FileOutputStream
import java.io.IOException
import java.io.InputStream
import java.io.OutputStream
import java.util.Properties
import java.util.zip.ZipEntry
import java.util.zip.ZipInputStream
import java.util.zip.ZipOutputStream

class SoundPackManager(context: Context) {
    private val appContext: Context = context.applicationContext
    private val store = SoundSettingsStore(appContext)

    @Throws(IOException::class)
    fun exportTo(uri: Uri) {
        val raw = appContext.contentResolver.openOutputStream(uri, "w")
            ?: throw IOException("Không thể tạo gói âm thanh")
        ZipOutputStream(BufferedOutputStream(raw)).use { zip ->
            val properties = Properties().apply {
                setProperty("format", "PingFootballSoundPack")
                setProperty("version", "1")
                setProperty("masterVolumePercent", store.volumePercent.toString())
            }
            for (slot in SoundCatalog.allSlots()) {
                if (store.isDisabled(slot.id)) {
                    properties.setProperty("slot.${slot.id}.state", "disabled")
                    continue
                }
                val source = store.resolve(slot)
                if (source == null) {
                    properties.setProperty("slot.${slot.id}.state", "empty")
                    continue
                }
                properties.setProperty("slot.${slot.id}.state", "audio")
                properties.setProperty("slot.${slot.id}.name", source.displayName)
                val entryName = "audio/${slot.id}.audio"
                properties.setProperty("slot.${slot.id}.file", entryName)
                zip.putNextEntry(ZipEntry(entryName))
                val input: InputStream = if (source.isAsset) {
                    appContext.assets.open(source.assetPath!!)
                } else {
                    FileInputStream(source.file!!)
                }
                try {
                    copy(input, zip, MAX_PACK_BYTES, closeOutput = false)
                } finally {
                    zip.closeEntry()
                }
            }
            zip.putNextEntry(ZipEntry(MANIFEST))
            properties.store(zip, "Ping Football audio pack")
            zip.closeEntry()
        }
    }

    @Throws(IOException::class)
    fun importFrom(uri: Uri) {
        val stage = File(appContext.cacheDir, "sound_pack_import_${System.nanoTime()}")
        if (!stage.mkdirs()) throw IOException("Không thể chuẩn bị nhập gói")
        val properties = Properties()
        try {
            extractValidated(uri, stage, properties)
            if (properties.getProperty("format") != "PingFootballSoundPack" ||
                properties.getProperty("version") != "1"
            ) {
                throw IOException("Gói âm thanh không đúng định dạng hoặc phiên bản")
            }
            validateManifest(properties, stage)
            store.resetAll()
            store.volumePercent = properties.getProperty("masterVolumePercent", "100").toIntOrNull() ?: 100
            for (slot in SoundCatalog.allSlots()) {
                val state = properties.getProperty("slot.${slot.id}.state", "empty")
                if (state == "disabled") {
                    store.setDisabled(slot.id, true)
                } else if (state == "audio") {
                    val relPath = properties.getProperty("slot.${slot.id}.file") ?: continue
                    val source = File(stage, relPath)
                    val directory = store.directory()
                    if (!directory.exists() && !directory.mkdirs()) {
                        throw IOException("Không thể tạo thư mục âm thanh")
                    }
                    copy(
                        FileInputStream(source),
                        FileOutputStream(store.customFile(slot.id)),
                        100L * 1024L * 1024L,
                        closeOutput = true
                    )
                    store.preferences().edit()
                        .putString("name_${slot.id}", properties.getProperty("slot.${slot.id}.name", "Âm thanh nhập"))
                        .apply()
                }
            }
        } finally {
            deleteTree(stage)
        }
    }

    @Throws(IOException::class)
    private fun extractValidated(uri: Uri, stage: File, properties: Properties) {
        val raw = appContext.contentResolver.openInputStream(uri)
            ?: throw IOException("Không thể đọc gói âm thanh")
        var total = 0L
        var entries = 0
        ZipInputStream(BufferedInputStream(raw)).use { zip ->
            val stagePath = stage.canonicalPath + File.separator
            val buffer = ByteArray(16384)
            while (true) {
                val entry = zip.nextEntry ?: break
                entries++
                if (entries > 100 || entry.isDirectory) {
                    zip.closeEntry()
                    continue
                }
                val name = entry.name
                if (name != MANIFEST && !ENTRY_PATTERN.matches(name)) {
                    throw IOException("Gói chứa đường dẫn không hợp lệ")
                }
                val target = File(stage, name)
                if (!target.canonicalPath.startsWith(stagePath)) {
                    throw IOException("Gói chứa đường dẫn không an toàn")
                }
                val parent = target.parentFile
                if (parent != null && !parent.exists() && !parent.mkdirs()) {
                    throw IOException("Không thể giải nén gói")
                }
                FileOutputStream(target).use { output ->
                    while (true) {
                        val read = zip.read(buffer)
                        if (read == -1) break
                        total += read
                        if (total > MAX_PACK_BYTES) throw IOException("Gói âm thanh vượt quá 500 MB")
                        output.write(buffer, 0, read)
                    }
                }
                zip.closeEntry()
            }
        }
        val manifest = File(stage, MANIFEST)
        if (!manifest.isFile) throw IOException("Gói thiếu tệp mô tả")
        FileInputStream(manifest).use { properties.load(it) }
    }

    companion object {
        private const val MANIFEST = "soundpack.properties"
        private const val MAX_PACK_BYTES = 500L * 1024L * 1024L
        private val ENTRY_PATTERN = Regex("audio/[a-z0-9_]+\\.audio")

        @Throws(IOException::class)
        private fun validateManifest(properties: Properties, stage: File) {
            for (slot in SoundCatalog.allSlots()) {
                val state = properties.getProperty("slot.${slot.id}.state", "empty")
                if (state != "audio" && state != "disabled" && state != "empty") {
                    throw IOException("Trạng thái không hợp lệ tại ${slot.id}")
                }
                if (state == "audio") {
                    val expected = "audio/${slot.id}.audio"
                    val path = properties.getProperty("slot.${slot.id}.file")
                    if (expected != path || !File(stage, path).isFile) {
                        throw IOException("Gói thiếu âm thanh ${slot.id}")
                    }
                }
            }
        }

        @Throws(IOException::class)
        private fun copy(input: InputStream, output: OutputStream, limit: Long, closeOutput: Boolean) {
            val buffer = ByteArray(16384)
            var total = 0L
            try {
                while (true) {
                    val read = input.read(buffer)
                    if (read == -1) break
                    total += read
                    if (total > limit) throw IOException("Dữ liệu âm thanh quá lớn")
                    output.write(buffer, 0, read)
                }
                output.flush()
            } finally {
                input.close()
                if (closeOutput) output.close()
            }
        }

        private fun deleteTree(file: File?) {
            if (file == null || !file.exists()) return
            file.listFiles()?.forEach { deleteTree(it) }
            file.delete()
        }
    }
}
