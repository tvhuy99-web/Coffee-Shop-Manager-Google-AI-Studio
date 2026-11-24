package android.util
import java.io.*
class AtomicFile(private val f:File){fun startWrite()=FileOutputStream(File(f.path+".new"));fun finishWrite(s:FileOutputStream){s.close();check(File(f.path+".new").renameTo(f))};fun failWrite(s:FileOutputStream){s.close();File(f.path+".new").delete()};fun readFully()=f.readBytes();fun delete(){f.delete()}}
