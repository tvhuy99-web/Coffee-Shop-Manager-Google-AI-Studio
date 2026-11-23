package org.json

/** Deliberately tiny JSON stub; real Android uses org.json.JSONObject. */
class JSONObject(private val contents: Map<String, Any?> = emptyMap()) {
    fun has(key: String): Boolean = contents.containsKey(key)
    fun optInt(key: String, fallback: Int = 0): Int = (contents[key] as? Number)?.toInt() ?: fallback
    fun optLong(key: String, fallback: Long = 0L): Long = (contents[key] as? Number)?.toLong() ?: fallback
    fun optString(key: String, fallback: String = ""): String = contents[key]?.toString() ?: fallback
    fun optJSONObject(key: String): JSONObject? = contents[key] as? JSONObject
}
