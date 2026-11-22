package pl.mkrstudio.truefootball3.lab

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.os.Build
import java.io.File
import java.io.PrintWriter
import java.io.StringWriter
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

object LabLogStore {
    const val MAX_CHARS = 500_000
    private const val PREFS = "tf3LabLogPrefs"
    private const val KEY_ENABLED = "log_enabled"
    private const val LOG_FILE_NAME = "lab_diagnostic_log.txt"
    private const val CLIPBOARD_SAFE_LIMIT = 450_000

    private val lock = Any()
    private val buffer = StringBuilder(32_768)
    private val timeFormat = SimpleDateFormat("HH:mm:ss.SSS", Locale.US)

    @Volatile private var initialized = false
    @Volatile private var enabled = true
    private var appContext: Context? = null
    private var logFile: File? = null
    private var linesSinceFlush = 0

    fun init(context: Context) {
        synchronized(lock) {
            if (initialized) return
            val ctx = context.applicationContext
            appContext = ctx
            val prefs = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            enabled = prefs.getBoolean(KEY_ENABLED, true)
            val file = File(ctx.filesDir, LOG_FILE_NAME)
            logFile = file
            if (file.isFile) {
                try {
                    val saved = file.readText(Charsets.UTF_8)
                    buffer.setLength(0)
                    buffer.append(saved)
                    trimLocked()
                } catch (_: Exception) {
                }
            }
            installCrashHandlerLocked()
            initialized = true
        }
        log(
            "SYSTEM",
            "App init | Android ${Build.VERSION.RELEASE} (API ${Build.VERSION.SDK_INT}) | " +
                "ABI=${Build.SUPPORTED_ABIS.joinToString(",")} | ${NativeBridge.status()}"
        )
    }

    private fun installCrashHandlerLocked() {
        val previous = Thread.getDefaultUncaughtExceptionHandler()
        Thread.setDefaultUncaughtExceptionHandler { thread, throwable ->
            try {
                error("CRASH", "Uncaught exception on thread ${thread.name}", throwable)
                flushToDisk()
            } catch (_: Throwable) {
            }
            previous?.uncaughtException(thread, throwable)
        }
    }

    fun isEnabled(): Boolean = enabled

    fun setEnabled(value: Boolean) {
        synchronized(lock) {
            enabled = value
            appContext?.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
                ?.edit()
                ?.putBoolean(KEY_ENABLED, value)
                ?.apply()
        }
        if (value) {
            log("SYSTEM", "Đã bật ghi nhật ký chi tiết (giới hạn $MAX_CHARS ký tự)")
        }
    }

    fun log(tag: String, message: String) {
        if (!enabled) return
        val line = synchronized(lock) {
            val ts = timeFormat.format(Date())
            val entry = "[$ts] [$tag] $message\n"
            buffer.append(entry)
            trimLocked()
            linesSinceFlush++
            val urgent = tag == "ERROR" || tag == "CRASH" || tag == "MATCH" || tag == "CALIBRATION"
            if (urgent || linesSinceFlush >= 25) {
                linesSinceFlush = 0
                writeFileLocked()
            }
            entry
        }
        @Suppress("UNUSED_VARIABLE")
        val unused = line
    }

    fun error(tag: String, message: String, throwable: Throwable? = null) {
        if (!enabled) return
        val full = if (throwable != null) {
            val sw = StringWriter()
            throwable.printStackTrace(PrintWriter(sw))
            "$message\n$sw"
        } else {
            message
        }
        log(tag, full.trimEnd())
    }

    fun clear() {
        synchronized(lock) {
            buffer.setLength(0)
            linesSinceFlush = 0
            try {
                logFile?.delete()
            } catch (_: Exception) {
            }
        }
    }

    fun length(): Int = synchronized(lock) { buffer.length }

    fun getFullText(): String = synchronized(lock) { buffer.toString() }

    fun getPreviewText(maxChars: Int = 35_000): String = synchronized(lock) {
        if (buffer.length <= maxChars) return buffer.toString()
        val start = (buffer.length - maxChars).coerceAtLeast(0)
        val nl = buffer.indexOf("\n", start)
        val cut = if (nl in start until buffer.length - 1) nl + 1 else start
        "... (đang hiển thị ${buffer.length - cut} ký tự mới nhất / tổng ${buffer.length} ký tự; bấm Sao chép để lấy đầy đủ) ...\n" +
            buffer.substring(cut)
    }

    fun flushToDisk() {
        synchronized(lock) {
            writeFileLocked()
        }
    }

    fun copyToClipboard(context: Context): Int {
        val text = getFullText()
        if (text.isEmpty()) return 0
        val cm = context.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager ?: return 0
        return try {
            cm.setPrimaryClip(ClipData.newPlainText("FootballMatchLabLog", text))
            text.length
        } catch (_: RuntimeException) {

            val start = (text.length - CLIPBOARD_SAFE_LIMIT).coerceAtLeast(0)
            val tail = text.substring(start)
            try {
                cm.setPrimaryClip(ClipData.newPlainText("FootballMatchLabLog", tail))
                tail.length
            } catch (_: RuntimeException) {
                0
            }
        }
    }

    private fun trimLocked() {
        if (buffer.length <= MAX_CHARS) return
        val excess = buffer.length - MAX_CHARS
        val nl = buffer.indexOf("\n", excess)
        val cut = if (nl != -1 && nl + 1 < buffer.length) nl + 1 else excess
        buffer.delete(0, cut)
    }

    private fun writeFileLocked() {
        val file = logFile ?: return
        try {
            file.writeText(buffer.toString(), Charsets.UTF_8)
        } catch (_: Exception) {
        }
    }
}
