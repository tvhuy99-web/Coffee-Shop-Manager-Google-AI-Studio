import android.content.Context
import android.content.Resources
import org.json.JSONObject
import pl.mkrstudio.truefootball3.lab.*
import java.io.File
import javax.xml.parsers.DocumentBuilderFactory

/** Full original XML resources + live Kotlin selector; no Java V3 or Android runtime. */
fun main(args: Array<String>) {
    require(args.size == 1) { "Expected path to Android main/res folder" }
    val res = File(args[0])
    val strings = linkedMapOf<String, String>()
    for (xml in listOf("commentary_v2.xml", "commentary_v3.xml")) {
        val file = File(res, "values-vi/$xml")
        val document = DocumentBuilderFactory.newInstance().newDocumentBuilder().parse(file)
        val elements = document.getElementsByTagName("string")
        for (i in 0 until elements.length) {
            val element = elements.item(i) as org.w3c.dom.Element
            strings[element.getAttribute("name")] = element.textContent
        }
    }
    check(strings.size == 895) { "Expected 895 genuine Vietnamese XML templates, got ${strings.size}" }
    for (xml in listOf("lab_commentary_fallbacks.xml")) {
        val document = DocumentBuilderFactory.newInstance().newDocumentBuilder().parse(File(res, "values-vi/$xml"))
        val elements = document.getElementsByTagName("string")
        for (i in 0 until elements.length) {
            val element = elements.item(i) as org.w3c.dom.Element
            strings[element.getAttribute("name")] = element.textContent
        }
    }
    val resources = Resources(strings)
    var tests = 0
    fun yes(ok: Boolean, message: String) { tests++; check(ok) { message } }

    fun render(kind: LabEventType, side: LabTeamSide?, actor: String = "", target: String = "",
               minute: Int = 1, scoreHome: Int = 0, scoreAway: Int = 0,
               meta: Map<String, Any?> = emptyMap(), seed: Long = 101L): String? {
        val engine = LabCommentaryEngine(Context(resources), "Chelsea FC", "Arsenal FC", seed)
        return engine.renderRustEvent(
            JSONObject(mapOf("gameSecond" to minute * 60, "sequence" to 5L,
                "ballX" to 45, "metadata" to JSONObject(meta), "tacticalPhase" to "PROGRESSION")),
            kind, side, actor, target, minute, scoreHome, scoreAway)
    }
    val actor = "Martin Odegaard"
    val first = render(LabEventType.FIRST_TOUCH, LabTeamSide.AWAY, actor = actor)
    yes(first != null && actor in first && "Chelsea FC" !in first,
        "FIRST_TOUCH must name Arsenal's receiver, not Chelsea: $first")
    val second = render(LabEventType.FIRST_TOUCH, LabTeamSide.HOME, actor = "Pedro Neto")
    yes(second != null && "Pedro Neto" in second && "Arsenal FC" !in second,
        "FIRST_TOUCH must name Chelsea's receiver: $second")
    yes(render(LabEventType.FIRST_TOUCH, LabTeamSide.AWAY) == null,
        "Missing receiver must not turn into a team name")

    for (seed in 1L..30L) {
        val start = render(LabEventType.MATCH_STARTED, null, seed = seed)
        yes(start != null && "trận đấu" in start.lowercase() ||
            (start != null && "hiệp đấu" in start.lowercase()), "Bad match start: $start")
        val kickoff = render(LabEventType.KICKOFF, LabTeamSide.AWAY, actor = "Viktor Gyokeres", seed = seed)
        yes(kickoff != null && "giao bóng mở màn" in kickoff, "Bad kickoff: $kickoff")
        val phase = render(LabEventType.TACTICAL_PHASE_CHANGED, LabTeamSide.HOME, minute = 4, seed = seed)
        yes(phase != null && !phase.contains("khai cuộc", ignoreCase = true) &&
            !phase.contains("quyết định", ignoreCase = true) &&
            !phase.contains("quan trọng", ignoreCase = true), "Unjustified first-half match phase: $phase")
        val pass = render(LabEventType.PASS_COMPLETED, LabTeamSide.HOME,
            actor = "Marc Cucurella", target = "Cole Palmer", minute = 28, seed = seed)
        yes(pass != null && !pass.contains("sau bàn thắng", ignoreCase = true) &&
            !pass.contains("sau pha lập công", ignoreCase = true) &&
            !pass.contains("đón bóng", ignoreCase = true),
            "Unfounded goal/reception commentary after pass: $pass")
        val failed = render(LabEventType.PASS_FAILED, LabTeamSide.AWAY,
            actor = "Declan Rice", target = "William Saliba", minute = 6, seed = seed)
        yes(failed != null && "chuyền" in failed.lowercase() &&
            !failed.contains("đột phá", ignoreCase = true) && !failed.contains("bị đoạt", ignoreCase = true),
            "PASS_FAILED described a different action: $failed")
        val loose = render(LabEventType.SECOND_BALL_WON, LabTeamSide.AWAY,
            actor = "Bukayo Saka", minute = 18, seed = seed)
        yes(loose != null && "giành được bóng hai" in loose &&
            !loose.contains("chưa có chủ", ignoreCase = true), "Already-won second ball still contested: $loose")
        val turnover = render(LabEventType.TURNOVER, LabTeamSide.AWAY,
            actor = "David Raya", minute = 5, seed = seed)
        yes(turnover != null && !turnover.contains("phản công", ignoreCase = true) &&
            !turnover.contains("nguy hiểm", ignoreCase = true), "Turnover claims unproven counter: $turnover")
        val tackle = render(LabEventType.TACKLE_WON, LabTeamSide.AWAY,
            actor = "Martin Odegaard", target = "Cole Palmer", minute = 9, seed = seed)
        yes(tackle != null && tackle.contains("tắc bóng thành công"), "Tackle win became an invented recovery: $tackle")
        val shot = render(LabEventType.SHOT_OFF_TARGET, LabTeamSide.AWAY,
            actor = "Viktor Gyokeres", minute = 2, seed = seed)
        yes(shot != null && !shot.contains("cột dọc", ignoreCase = true) &&
            !shot.contains("xà ngang", ignoreCase = true), "Unknown miss direction was invented: $shot")
        val distribution = render(LabEventType.GOALKEEPER_DISTRIBUTION, LabTeamSide.HOME,
            actor = "Robert Sanchez", target = "Levi Colwill", minute = 10, seed = seed)
        yes(distribution != null && !distribution.contains("phản công", ignoreCase = true),
            "Goalkeeper pass was invented as a counter: $distribution")
    }

    // A stateful series rather than independent calls: a goalkeeper takes the ball.
    val e = LabCommentaryEngine(Context(resources), "Chelsea FC", "Arsenal FC", 17L)
    fun step(kind: LabEventType, actor: String, minute: Int, seq: Long, side: LabTeamSide = LabTeamSide.HOME) =
        e.renderRustEvent(JSONObject(mapOf("gameSecond" to minute * 60, "sequence" to seq,
            "ballX" to 45, "metadata" to JSONObject())), kind, side, actor, "", minute, 0, 0)
    step(LabEventType.GOALKEEPER_CLAIM, "Robert Sanchez", 11, 71L)
    val keeper = step(LabEventType.POSSESSION_STARTED, "Robert Sanchez", 11, 72L)
    yes(keeper != null && keeper.contains("bắt gọn bóng") && !keeper.contains("trung lộ"),
        "Goalkeeper claim was narrated as midfield possession: $keeper")

    val goal = render(LabEventType.GOAL, LabTeamSide.HOME, "Cole Palmer", minute = 30,
        scoreHome = 1, scoreAway = 0)
    yes(goal != null && ("Vàooo" in goal || "bàn" in goal.lowercase()),
        "A legitimate goal must still be announced: $goal")

    // 20 seed variants of a deliberate metadata switch: should not be discarded.
    for (seed in 31L..50L) {
        val sw = render(LabEventType.PASS_COMPLETED, LabTeamSide.AWAY, "Martin Odegaard",
            "Bukayo Saka", minute = 22, meta = mapOf("passType" to "SWITCH"), seed = seed)
        yes(sw != null && (sw.contains("cánh") || sw.contains("biên") || sw.contains("hướng")),
            "Real SWITCH evidence lost: $sw")
    }
    // Factual fallbacks must be localized, even though they are not in legacy cv2/cv3 XML.
    val englishStrings = linkedMapOf<String, String>()
    for (xml in listOf("commentary_v2.xml", "commentary_v3.xml", "lab_commentary_fallbacks.xml")) {
        val document = DocumentBuilderFactory.newInstance().newDocumentBuilder().parse(File(res, "values/$xml"))
        val elements = document.getElementsByTagName("string")
        for (i in 0 until elements.length) {
            val element = elements.item(i) as org.w3c.dom.Element
            englishStrings[element.getAttribute("name")] = element.textContent
        }
    }
    yes(englishStrings.size == strings.size, "VI/EN resource keys differ")
    val english = LabCommentaryEngine(Context(Resources(englishStrings)), "Chelsea FC", "Arsenal FC", 7L)
    fun en(kind: LabEventType, actor: String, side: LabTeamSide = LabTeamSide.AWAY): String? =
        english.renderRustEvent(JSONObject(mapOf("gameSecond" to 360, "sequence" to 16L,
            "metadata" to JSONObject())), kind, side, actor, "", 6, 0, 0)
    yes(en(LabEventType.PASS_FAILED, "Declan Rice") == "Declan Rice fails to complete the pass.",
        "English event should not receive Vietnamese fallback")
    yes(en(LabEventType.SECOND_BALL_WON, "Bukayo Saka") == "Bukayo Saka wins the second ball for Arsenal FC.",
        "English second-ball fallback incorrect")

    val penalty = render(LabEventType.PENALTY_SHOOTOUT_KICK, LabTeamSide.HOME,
        actor = "Cole Palmer", minute = 121, meta = mapOf("scored" to "true"))
    yes(penalty != null && penalty.contains("ghi bàn") && !penalty.contains("0-0"),
        "Penalty kick outcome must be spoken, not fake '0-0' metadata: $penalty")

    println("PASS: $tests genuine-XML Kotlin commentary regressions")
}
