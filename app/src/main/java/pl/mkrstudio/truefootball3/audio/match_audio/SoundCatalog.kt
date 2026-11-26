package pl.mkrstudio.truefootball3.match_audio

import pl.mkrstudio.truefootball3.audio.*

import java.util.Collections
import java.util.LinkedHashMap

object SoundCatalog {
    const val ASSET_ROOT = "mod_audio/core/"

    class Slot internal constructor(
        @JvmField val id: String,
        @JvmField val group: String,
        @JvmField val label: String,
        @JvmField val defaultAsset: String?
    )

    class Group internal constructor(
        @JvmField val name: String,
        slots: List<Slot>
    ) {
        @JvmField
        val slots: List<Slot> = Collections.unmodifiableList(slots)
    }

    private val GROUPS: List<Group>
    private val BY_ID: Map<String, Slot>

    init {
        val list = ArrayList<Group>()
        addCapacityGroup(list, "Âm thanh nền sân vận động", "stadium", "stadium_ambience.ogg")
        addVariants(list, "Khán giả dâng cao khi tấn công/phản công", "attack_rise", "attack_rise.ogg")
        addVariants(list, "Chuyền bóng / chạm bóng nhẹ", "pass_kick", "ball_kick.ogg")
        addVariants(list, "Sút bóng mạnh", "strong_shot", "ball_kick.ogg")
        addVariants(list, "Đánh đầu", "header", "ball_kick.ogg")
        addVariants(list, "Bóng chạm lưới", "net_hit", "net_hit.ogg")
        addVariants(list, "Bóng chạm cột dọc / xà ngang", "post_crossbar", null)
        addVariants(list, "Cú sút suýt ghi bàn", "near_miss", "near_miss.ogg")
        addVariants(list, "Thủ môn cứu bóng", "keeper_save", "keeper_save.ogg")
        addCapacityGroup(list, "Ghi bàn sân nhà", "home_goal", "home_goal.ogg")
        addCapacityGroup(list, "Ghi bàn sân khách", "away_goal", "away_goal_reaction.ogg")
        addSingle(list, "Còi trọng tài dùng chung", "referee_whistle", "Âm thanh còi", "referee_whistle.ogg")
        addVariants(list, "Vỗ tay khi thay người", "substitution", null)
        addVariants(list, "Khán giả ăn mừng sau trận", "post_match", null)

        val map = LinkedHashMap<String, Slot>()
        for (group in list) {
            for (slot in group.slots) {
                map[slot.id] = slot
            }
        }
        GROUPS = Collections.unmodifiableList(list)
        BY_ID = Collections.unmodifiableMap(map)
    }

    private fun addCapacityGroup(groups: MutableList<Group>, name: String, prefix: String, defaultFile: String?) {
        val slots = listOf(
            slot("${prefix}_small", name, "Sân dưới 30.000 chỗ", defaultFile),
            slot("${prefix}_medium", name, "Sân từ 30.000 đến 60.000 chỗ", defaultFile),
            slot("${prefix}_large", name, "Sân trên 60.000 chỗ", defaultFile)
        )
        groups.add(Group(name, slots))
    }

    private fun addVariants(groups: MutableList<Group>, name: String, prefix: String, defaultFile: String?) {
        val slots = (1..3).map { i ->
            slot("${prefix}_$i", name, "Biến thể $i", if (i == 1) defaultFile else null)
        }
        groups.add(Group(name, slots))
    }

    private fun addSingle(groups: MutableList<Group>, name: String, id: String, label: String, defaultFile: String?) {
        groups.add(Group(name, listOf(slot(id, name, label, defaultFile))))
    }

    @Suppress("UNUSED_PARAMETER")
    private fun slot(id: String, group: String, label: String, defaultFile: String?): Slot =
        Slot(id, group, label, "$ASSET_ROOT$id.ogg")

    @JvmStatic
    fun groups(): List<Group> = GROUPS

    @JvmStatic
    fun find(id: String?): Slot? = if (id == null) null else BY_ID[id]

    @JvmStatic
    fun allSlots(): List<Slot> = ArrayList(BY_ID.values)

    @JvmStatic
    fun capacitySuffix(capacity: Int): String = when {
        capacity < 30000 -> "small"
        capacity <= 60000 -> "medium"
        else -> "large"
    }
}
