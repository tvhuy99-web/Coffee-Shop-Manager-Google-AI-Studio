package android.util
import java.io.*
class AtomicFile(private val base: File) {
    fun startWrite(): FileOutputStream = FileOutputStream(File(base.path + ".new"))
    fun finishWrite(stream: FileOutputStream) { stream.fd.sync(); stream.close(); check(File(base.path + ".new").renameTo(base)) }
    fun failWrite(stream: FileOutputStream) { stream.close(); File(base.path + ".new").delete() }
    fun readFully(): ByteArray = base.readBytes()
}
