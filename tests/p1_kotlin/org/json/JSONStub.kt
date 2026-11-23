package org.json

/** JVM-only stand-in for verifying store/JNI wrapper semantics without Android SDK.
 * Values live in a process-local registry; serialized tokens are NOT real JSON.
 */
private object FakeJsonRegistry {
    val objects = linkedMapOf<String, Map<String, Any?>>()
    val arrays = linkedMapOf<String, List<Any?>>()
    var number = 0
    fun next(prefix: String) = prefix + (++number).toString()
}

class JSONObject {
    private val values = linkedMapOf<String, Any?>()
    constructor()
    constructor(token: String) {
        if (token == "{}") return
        values.putAll(FakeJsonRegistry.objects[token] ?: error("Unknown fake JSON object: $token"))
    }
    fun put(name: String, value: Any?): JSONObject { values[name] = value; return this }
    fun getString(name: String): String = values[name] as String
    fun getLong(name: String): Long = (values[name] as Number).toLong()
    fun optString(name: String, fallback: String = ""): String = values[name]?.toString() ?: fallback
    fun optInt(name: String, fallback: Int = 0): Int = (values[name] as? Number)?.toInt() ?: fallback
    fun optBoolean(name: String, fallback: Boolean = false): Boolean = values[name] as? Boolean ?: fallback
    fun optJSONObject(name: String): JSONObject? = values[name] as? JSONObject
    fun optJSONArray(name: String): JSONArray? = values[name] as? JSONArray
    override fun toString(): String {
        val token = FakeJsonRegistry.next("OJ:")
        FakeJsonRegistry.objects[token] = LinkedHashMap(values)
        return token
    }
}

class JSONArray {
    private val values = arrayListOf<Any?>()
    constructor()
    constructor(serialized: String) {
        if (serialized == "[]") return
        values.addAll(FakeJsonRegistry.arrays[serialized] ?: error("Unknown fake JSON array: $serialized"))
    }
    fun put(value: Any?): JSONArray { values.add(value); return this }
    fun length() = values.size
    fun getJSONObject(index: Int): JSONObject = values[index] as JSONObject
    override fun toString(): String {
        val token = FakeJsonRegistry.next("AJ:")
        FakeJsonRegistry.arrays[token] = ArrayList(values)
        return token
    }
}
