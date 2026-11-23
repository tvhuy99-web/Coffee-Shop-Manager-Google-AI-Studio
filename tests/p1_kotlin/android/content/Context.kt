package android.content
import java.io.File
open class Context(val filesDir: File = File(System.getProperty("java.io.tmpdir"))) { val applicationContext: Context get() = this }
