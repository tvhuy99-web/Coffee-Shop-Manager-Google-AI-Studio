package android.content

/** Minimal JVM replacement for running commentary tests without an Android device. */
class Resources(private val strings: Map<String, String>) {
    private val keys = strings.keys.toList()
    fun getIdentifier(name: String, type: String, pkg: String): Int = keys.indexOf(name) + 1
    fun getString(id: Int): String = strings.getValue(keys[id - 1])
}
open class Context(val resources: Resources, val packageName: String = "commentary.jvm") {
    fun getString(id: Int): String = resources.getString(id)
}
