package pl.mkrstudio.truefootball3.activities

import android.app.Activity
import android.content.Intent
import android.graphics.Color
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.speech.tts.TextToSpeech
import android.speech.tts.UtteranceProgressListener
import android.speech.tts.Voice
import android.view.Gravity
import android.view.View
import android.widget.AdapterView
import android.widget.ArrayAdapter
import android.widget.Button
import android.widget.CheckBox
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.SeekBar
import android.widget.Spinner
import android.widget.TextView
import android.widget.Toast
import java.util.LinkedHashMap
import java.util.Locale
import pl.mkrstudio.truefootball3.R
import pl.mkrstudio.truefootball3.audio.TtsSettingsStore

class TtsSettingsActivity : Activity() {
    private val handler = Handler(Looper.getMainLooper())
    private val engines = ArrayList<EngineOption>()
    private val languages = ArrayList<LanguageOption>()
    private val voices = ArrayList<VoiceOption>()

    private lateinit var store: TtsSettingsStore
    private lateinit var draft: TtsSettingsStore.Selection
    private lateinit var engineSpinner: Spinner
    private lateinit var languageSpinner: Spinner
    private lateinit var voiceSpinner: Spinner
    private lateinit var enabledCheckBox: CheckBox
    private lateinit var volumeSeekBar: SeekBar
    private lateinit var volumeLabel: TextView
    private lateinit var rateSeekBar: SeekBar
    private lateinit var rateLabel: TextView
    private lateinit var pitchSeekBar: SeekBar
    private lateinit var pitchLabel: TextView
    private lateinit var statusText: TextView
    private lateinit var previewButton: Button
    private lateinit var saveButton: Button

    private var catalogTts: TextToSpeech? = null
    private var generation = 0
    private var currentEngineIndex = -1
    private var currentLanguageIndex = -1
    private var currentVoiceIndex = -1
    private var engineTimeout: Runnable? = null
    private var previewMonitor: Runnable? = null
    private var activePreviewUtterance: String? = null
    private var previewObservedSpeaking = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        requestWindowFeature(1)
        store = TtsSettingsStore(this)
        draft = store.load()
        setContentView(buildUi())
        discoverEngines()
    }

    private fun buildUi(): View {
        val outer = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(18), dp(14), dp(18), dp(14))
            setBackgroundResource(R.drawable.main_background)
        }

        val title = label("Cài đặt giọng đọc trận đấu", 23).apply {
            gravity = Gravity.CENTER
            contentDescription = "Tiêu đề. Cài đặt giọng đọc trận đấu"
        }
        outer.addView(title, fullWrap())

        val content = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
        }

        enabledCheckBox = CheckBox(this).apply {
            text = "Đọc nội dung bình luận trận đấu"
            setTextColor(Color.WHITE)
            textSize = 17f
            minHeight = dp(52)
            isChecked = draft.enabled
        }
        content.addView(enabledCheckBox, fullWrap())

        engineSpinner = spinner()
        addField(content, "Bộ đọc", engineSpinner)
        languageSpinner = spinner()
        addField(content, "Ngôn ngữ", languageSpinner)
        voiceSpinner = spinner()
        addField(content, "Giọng đọc", voiceSpinner)

        volumeSeekBar = SeekBar(this).apply {
            id = View.generateViewId()
            max = 100
            progress = draft.volumePercent
            minimumHeight = dp(52)
        }
        volumeLabel = label("", 17).apply {
            labelFor = volumeSeekBar.id
            setPadding(0, dp(14), 0, dp(4))
        }
        content.addView(volumeLabel, fullWrap())
        content.addView(volumeSeekBar, fullWrap())
        updateVolumeLabel(draft.volumePercent)
        volumeSeekBar.setOnSeekBarChangeListener(simpleSeekListener { progress ->
            updateVolumeLabel(progress)
            updateDraft()
        })

        rateSeekBar = SeekBar(this).apply {
            id = View.generateViewId()
            max = 450
            progress = draft.ratePercent - 50
            minimumHeight = dp(52)
        }
        rateLabel = label("", 17).apply {
            labelFor = rateSeekBar.id
            setPadding(0, dp(14), 0, dp(4))
        }
        content.addView(rateLabel, fullWrap())
        content.addView(rateSeekBar, fullWrap())
        updateRateLabel(draft.ratePercent)
        rateSeekBar.setOnSeekBarChangeListener(simpleSeekListener { progress ->
            updateRateLabel(progress + 50)
            updateDraft()
        })

        pitchSeekBar = SeekBar(this).apply {
            id = View.generateViewId()
            max = 150
            progress = draft.pitchPercent - 50
            minimumHeight = dp(52)
        }
        pitchLabel = label("", 17).apply {
            labelFor = pitchSeekBar.id
            setPadding(0, dp(14), 0, dp(4))
        }
        content.addView(pitchLabel, fullWrap())
        content.addView(pitchSeekBar, fullWrap())
        updatePitchLabel(draft.pitchPercent)
        pitchSeekBar.setOnSeekBarChangeListener(simpleSeekListener { progress ->
            updatePitchLabel(progress + 50)
            updateDraft()
        })

        statusText = label("Đang tải…", 14).apply {
            setPadding(0, dp(18), 0, dp(10))
        }
        content.addView(statusText, fullWrap())

        previewButton = button("Nghe thử") { previewSelection() }.apply {
            isEnabled = false
        }
        content.addView(previewButton, fullWrap())

        enabledCheckBox.setOnCheckedChangeListener { _, _ -> updateDraft() }

        val scroll = ScrollView(this).apply {
            isFillViewport = true
            addView(content, fullWrap())
        }
        outer.addView(scroll, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, 0, 1f))

        val actions = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            setPadding(0, dp(8), 0, 0)
        }
        saveButton = button("Lưu TTS") { saveSelection() }.apply {
            isEnabled = false
        }
        actions.addView(saveButton, weighted())
        actions.addView(button("Đóng") { finish() }, weighted())
        outer.addView(actions, fullWrap())
        return outer
    }

    private fun simpleSeekListener(onChange: (Int) -> Unit) = object : SeekBar.OnSeekBarChangeListener {
        override fun onStartTrackingTouch(seekBar: SeekBar?) {}
        override fun onStopTrackingTouch(seekBar: SeekBar?) {}
        override fun onProgressChanged(seekBar: SeekBar?, progress: Int, fromUser: Boolean) {
            onChange(progress)
        }
    }

    private fun discoverEngines() {
        engines.clear()
        engines.add(EngineOption("", "Mặc định hệ thống"))
        val discovered = LinkedHashMap<String, String>()
        val pm = packageManager
        try {
            val intent = Intent(TextToSpeech.Engine.INTENT_ACTION_TTS_SERVICE)
            val services = pm.queryIntentServices(intent, 0)
            for (info in services) {
                val pkg = info.serviceInfo?.packageName ?: continue
                val appLabel = info.loadLabel(pm)?.toString() ?: pkg
                discovered[pkg] = "$appLabel • $pkg"
            }
        } catch (_: RuntimeException) {
        }
        discovered.entries
            .sortedWith { left, right -> left.value.compareTo(right.value, ignoreCase = true) }
            .forEach { engines.add(EngineOption(it.key, it.value)) }

        engineSpinner.adapter = adapter(engines.map { it.label })
        val selected = findEngine(draft.enginePackage)
        currentEngineIndex = selected
        engineSpinner.setSelection(selected, false)
        engineSpinner.isEnabled = true
        describe(engineSpinner, "Bộ đọc TTS", engines[selected].label)
        engineSpinner.onItemSelectedListener = object : AdapterView.OnItemSelectedListener {
            override fun onNothingSelected(parent: AdapterView<*>?) {}
            override fun onItemSelected(parent: AdapterView<*>?, view: View?, position: Int, id: Long) {
                if (position == currentEngineIndex || position !in engines.indices) return
                currentEngineIndex = position
                val option = engines[position]
                draft = TtsSettingsStore.Selection(
                    enabledCheckBox.isChecked,
                    option.packageName,
                    TtsSettingsStore.DEFAULT_LANGUAGE,
                    "",
                    volumeSeekBar.progress,
                    voiceControlPercent(rateSeekBar),
                    voiceControlPercent(pitchSeekBar)
                )
                describe(engineSpinner, "Bộ đọc TTS", option.label)
                inspectEngine(option, draft.languageTag, draft.voiceName)
            }
        }
        inspectEngine(engines[selected], draft.languageTag, draft.voiceName)
    }

    private fun inspectEngine(option: EngineOption, wantedLanguage: String, wantedVoice: String) {
        val request = ++generation
        shutdownCatalog()
        languages.clear()
        voices.clear()
        languageSpinner.isEnabled = false
        voiceSpinner.isEnabled = false
        previewButton.isEnabled = false
        saveButton.isEnabled = false
        status("Đang tải bộ đọc…", error = false)

        var holder: TextToSpeech? = null
        val listener = TextToSpeech.OnInitListener { result ->
            handler.post {
                val engine = holder
                if (request != generation || engine == null || engine !== catalogTts) return@post
                cancelEngineTimeout()
                if (result != TextToSpeech.SUCCESS) {
                    engineFailure("Không khởi tạo được bộ đọc này.")
                    return@post
                }
                buildCatalog(request, engine, wantedLanguage, wantedVoice)
            }
        }
        try {
            holder = if (option.packageName.isEmpty()) {
                TextToSpeech(this, listener)
            } else {
                TextToSpeech(this, listener, option.packageName)
            }
            catalogTts = holder
        } catch (_: RuntimeException) {
            engineFailure("Không khởi tạo được bộ đọc này.")
            return
        }
        engineTimeout = Runnable {
            if (request == generation) engineFailure("Bộ đọc không phản hồi.")
        }.also { handler.postDelayed(it, ENGINE_TIMEOUT_MS) }
    }

    private fun buildCatalog(
        request: Int,
        engine: TextToSpeech,
        wantedLanguage: String,
        wantedVoice: String
    ) {
        Thread({
            val foundLanguages = collectLanguages(engine)
            handler.post {
                if (request != generation || engine !== catalogTts || isFinishing) return@post
                languages.clear()
                languages.addAll(foundLanguages)
                if (languages.isEmpty()) {
                    engineFailure("Bộ đọc này không có ngôn ngữ khả dụng.")
                    return@post
                }
                bindLanguages(wantedLanguage, wantedVoice)
                status("Sẵn sàng", error = false)
                previewButton.isEnabled = true
                saveButton.isEnabled = true
            }
        }, "tts-catalog").start()
    }

    private fun collectLanguages(engine: TextToSpeech): List<LanguageOption> {
        val byTag = LinkedHashMap<String, Locale>()
        try {
            engine.availableLanguages?.forEach { addLocale(byTag, it) }
        } catch (_: RuntimeException) {
        }
        try {
            engine.voices?.forEach { addLocale(byTag, it.locale) }
        } catch (_: RuntimeException) {
        }
        val vietnamese = Locale.forLanguageTag(TtsSettingsStore.DEFAULT_LANGUAGE)
        try {
            if (engine.isLanguageAvailable(vietnamese) >= TextToSpeech.LANG_AVAILABLE) {
                addLocale(byTag, vietnamese)
            }
        } catch (_: RuntimeException) {
        }
        if (byTag.isEmpty()) addLocale(byTag, Locale.getDefault())

        val displayLocale = Locale.forLanguageTag("vi-VN")
        return byTag.entries.map { (tag, locale) ->
            val human = locale.getDisplayName(displayLocale).ifEmpty { tag }
            LanguageOption(tag, locale, "$human • $tag")
        }.sortedWith { left, right ->
            val leftRank = languageRank(left.tag)
            val rightRank = languageRank(right.tag)
            if (leftRank != rightRank) leftRank - rightRank
            else left.label.compareTo(right.label, ignoreCase = true)
        }
    }

    private fun bindLanguages(wantedLanguage: String, wantedVoice: String) {
        languageSpinner.adapter = adapter(languages.map { it.label })
        val selected = findLanguage(wantedLanguage)
        currentLanguageIndex = selected
        languageSpinner.setSelection(selected, false)
        languageSpinner.isEnabled = true
        describe(languageSpinner, "Ngôn ngữ TTS", languages[selected].label)
        languageSpinner.onItemSelectedListener = object : AdapterView.OnItemSelectedListener {
            override fun onNothingSelected(parent: AdapterView<*>?) {}
            override fun onItemSelected(parent: AdapterView<*>?, view: View?, position: Int, id: Long) {
                if (position == currentLanguageIndex || position !in languages.indices) return
                currentLanguageIndex = position
                val language = languages[position]
                describe(languageSpinner, "Ngôn ngữ TTS", language.label)
                bindVoices(language, "")
                updateDraft()
            }
        }
        bindVoices(languages[selected], wantedVoice)
    }

    private fun bindVoices(language: LanguageOption, wantedVoice: String) {
        voices.clear()
        voices.add(VoiceOption(null, "Mặc định của bộ đọc"))
        try {
            val available = catalogTts?.voices
            if (available != null) {
                for (voice in available) {
                    if (language.tag.equals(normalizedTag(voice.locale), ignoreCase = true)) {
                        var value = voice.name
                        if (voice.isNetworkConnectionRequired) value += " • cần mạng"
                        voices.add(VoiceOption(voice, value))
                    }
                }
            }
        } catch (_: RuntimeException) {
        }
        if (voices.size > 2) {
            voices.subList(1, voices.size).sortWith { left, right ->
                left.label.compareTo(right.label, ignoreCase = true)
            }
        }
        voiceSpinner.adapter = adapter(voices.map { it.label })
        val selected = findVoice(wantedVoice)
        currentVoiceIndex = selected
        voiceSpinner.setSelection(selected, false)
        voiceSpinner.isEnabled = true
        describe(voiceSpinner, "Giọng đọc TTS", voices[selected].label)
        voiceSpinner.onItemSelectedListener = object : AdapterView.OnItemSelectedListener {
            override fun onNothingSelected(parent: AdapterView<*>?) {}
            override fun onItemSelected(parent: AdapterView<*>?, view: View?, position: Int, id: Long) {
                if (position == currentVoiceIndex || position !in voices.indices) return
                currentVoiceIndex = position
                describe(voiceSpinner, "Giọng đọc TTS", voices[position].label)
                updateDraft()
            }
        }
        updateDraft()
    }

    private fun previewSelection() {
        updateDraft()
        val engine = catalogTts
        if (engine == null || !applyDraft(engine)) {
            status("Không thể nghe thử lựa chọn này.", error = true)
            return
        }
        cancelPreviewMonitor()
        previewButton.isEnabled = false
        status("Đang nghe thử…", error = false)
        val utterance = "tts_preview_${System.currentTimeMillis()}"
        activePreviewUtterance = utterance
        previewObservedSpeaking = false
        engine.setOnUtteranceProgressListener(object : UtteranceProgressListener() {
            override fun onStart(id: String) {
                if (utterance == id) previewObservedSpeaking = true
            }

            override fun onDone(id: String) {
                finishPreview(id, utterance, true)
            }

            @Deprecated("Deprecated in Java")
            override fun onError(id: String) {
                finishPreview(id, utterance, false)
            }

            override fun onError(id: String, errorCode: Int) {
                finishPreview(id, utterance, false)
            }
        })
        val parameters = Bundle().apply {
            putFloat(TextToSpeech.Engine.KEY_PARAM_VOLUME, draft.volumePercent / 100f)
        }
        val result = try {
            engine.speak("Đây là giọng đọc bình luận trận đấu.", TextToSpeech.QUEUE_FLUSH, parameters, utterance)
        } catch (_: RuntimeException) {
            TextToSpeech.ERROR
        }
        if (result == TextToSpeech.ERROR) {
            finishPreview(utterance, utterance, false)
        } else {
            startPreviewMonitor(engine, utterance)
        }
    }

    private fun startPreviewMonitor(engine: TextToSpeech, utterance: String) {
        val startedAt = SystemClock.elapsedRealtime()
        val monitor = object : Runnable {
            override fun run() {
                if (utterance != activePreviewUtterance || isFinishing) return
                val speaking = try {
                    engine.isSpeaking
                } catch (_: RuntimeException) {
                    false
                }
                if (speaking) previewObservedSpeaking = true
                val elapsed = SystemClock.elapsedRealtime() - startedAt
                if ((!speaking && previewObservedSpeaking && elapsed >= 500L) || elapsed >= previewFallbackMs()) {
                    finishPreview(utterance, utterance, true)
                    return
                }
                handler.postDelayed(this, 250L)
            }
        }
        previewMonitor = monitor
        handler.postDelayed(monitor, 350L)
    }

    private fun previewFallbackMs(): Long {
        val rate = if (::draft.isInitialized) maxOf(50, draft.ratePercent) else 100
        return (8000L * 100L / rate).coerceIn(2500L, 15000L)
    }

    private fun cancelPreviewMonitor() {
        previewMonitor?.let { handler.removeCallbacks(it) }
        previewMonitor = null
    }

    private fun finishPreview(id: String?, expected: String, success: Boolean) {
        if (expected != id || expected != activePreviewUtterance) return
        activePreviewUtterance = null
        cancelPreviewMonitor()
        handler.post {
            if (isFinishing) return@post
            previewButton.isEnabled = catalogTts != null
            status(if (success) "Sẵn sàng" else "Nghe thử thất bại.", error = !success)
        }
    }

    private fun applyDraft(engine: TextToSpeech): Boolean {
        return try {
            if (engine.setSpeechRate(draft.ratePercent / 100f) == TextToSpeech.ERROR) return false
            if (engine.setPitch(draft.pitchPercent / 100f) == TextToSpeech.ERROR) return false
            val language = engine.setLanguage(Locale.forLanguageTag(draft.languageTag))
            if (language < TextToSpeech.LANG_AVAILABLE) return false
            if (draft.voiceName.isNotEmpty()) {
                for (voice in engine.voices.orEmpty()) {
                    if (draft.voiceName == voice.name) {
                        engine.setVoice(voice)
                        break
                    }
                }
            }
            true
        } catch (_: RuntimeException) {
            false
        }
    }

    private fun saveSelection() {
        updateDraft()
        store.save(draft)
        Toast.makeText(this, "Đã lưu cài đặt giọng đọc", Toast.LENGTH_SHORT).show()
        status("Đã lưu", error = false)
    }

    private fun updateDraft() {
        if (!::enabledCheckBox.isInitialized || !::volumeSeekBar.isInitialized ||
            !::rateSeekBar.isInitialized || !::pitchSeekBar.isInitialized
        ) return
        val engine = if (currentEngineIndex in engines.indices) engines[currentEngineIndex].packageName else draft.enginePackage
        val language = if (currentLanguageIndex in languages.indices) languages[currentLanguageIndex].tag else draft.languageTag
        val voice = if (currentVoiceIndex in voices.indices) voices[currentVoiceIndex].voice?.name ?: "" else ""
        draft = TtsSettingsStore.Selection(
            enabledCheckBox.isChecked,
            engine,
            language,
            voice,
            volumeSeekBar.progress,
            voiceControlPercent(rateSeekBar),
            voiceControlPercent(pitchSeekBar)
        )
    }

    private fun updateVolumeLabel(progress: Int) {
        if (!::volumeLabel.isInitialized || !::volumeSeekBar.isInitialized) return
        volumeLabel.text = "Âm lượng giọng đọc: $progress%"
        volumeSeekBar.contentDescription = "Âm lượng giọng đọc $progress phần trăm"
    }

    private fun updateRateLabel(percent: Int) {
        if (!::rateLabel.isInitialized || !::rateSeekBar.isInitialized) return
        rateLabel.text = "Tốc độ giọng đọc: $percent%"
        rateSeekBar.contentDescription = "Tốc độ giọng đọc $percent phần trăm"
    }

    private fun updatePitchLabel(percent: Int) {
        if (!::pitchLabel.isInitialized || !::pitchSeekBar.isInitialized) return
        pitchLabel.text = "Độ cao giọng đọc: $percent%"
        pitchSeekBar.contentDescription = "Độ cao giọng đọc $percent phần trăm"
    }

    private fun voiceControlPercent(seekBar: SeekBar?): Int = (seekBar?.progress ?: 50) + 50

    private fun engineFailure(message: String) {
        shutdownCatalog()
        languageSpinner.isEnabled = false
        voiceSpinner.isEnabled = false
        previewButton.isEnabled = false
        saveButton.isEnabled = false
        status(message, error = true)
    }

    private fun status(message: String, error: Boolean) {
        statusText.text = message
        statusText.setTextColor(if (error) 0xffff7777.toInt() else Color.WHITE)
        statusText.contentDescription = message
    }

    private fun shutdownCatalog() {
        cancelEngineTimeout()
        activePreviewUtterance = null
        cancelPreviewMonitor()
        catalogTts?.let {
            try {
                it.stop()
            } catch (_: RuntimeException) {
            }
            try {
                it.shutdown()
            } catch (_: RuntimeException) {
            }
            catalogTts = null
        }
    }

    private fun cancelEngineTimeout() {
        engineTimeout?.let { handler.removeCallbacks(it) }
        engineTimeout = null
    }

    override fun onDestroy() {
        generation++
        handler.removeCallbacksAndMessages(null)
        shutdownCatalog()
        super.onDestroy()
    }

    private fun addField(content: LinearLayout, text: String, target: Spinner) {
        val fieldLabel = label(text, 17).apply {
            setPadding(0, dp(14), 0, dp(4))
            labelFor = target.id
        }
        content.addView(fieldLabel, fullWrap())
        content.addView(target, fullWrap())
    }

    private fun spinner(): Spinner = Spinner(this).apply {
        id = View.generateViewId()
        minimumHeight = dp(52)
        isEnabled = false
    }

    private fun label(text: String, sp: Int): TextView = TextView(this).apply {
        this.text = text
        textSize = sp.toFloat()
        setTextColor(Color.WHITE)
    }

    private fun button(text: String, onClick: () -> Unit): Button = Button(this).apply {
        this.text = text
        isAllCaps = false
        minHeight = dp(52)
        setOnClickListener { onClick() }
    }

    private fun adapter(values: List<String>): ArrayAdapter<String> =
        ArrayAdapter(this, android.R.layout.simple_spinner_item, values).apply {
            setDropDownViewResource(android.R.layout.simple_spinner_dropdown_item)
        }

    private fun describe(view: View, field: String, value: String) {
        view.contentDescription = "$field. $value"
    }

    private fun findEngine(packageName: String): Int =
        engines.indexOfFirst { it.packageName == packageName }.let { if (it >= 0) it else 0 }

    private fun findLanguage(wanted: String): Int {
        val safe = TtsSettingsStore.normalizeLanguageTag(wanted)
        val exact = languages.indexOfFirst { it.tag.equals(safe, ignoreCase = true) }
        if (exact >= 0) return exact
        val vi = languages.indexOfFirst { it.tag.lowercase(Locale.ROOT).startsWith("vi") }
        return if (vi >= 0) vi else 0
    }

    private fun findVoice(wanted: String?): Int {
        if (wanted.isNullOrEmpty()) return 0
        for (i in 1 until voices.size) {
            if (voices[i].voice?.name == wanted) return i
        }
        return 0
    }

    private fun fullWrap(): LinearLayout.LayoutParams =
        LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT)

    private fun weighted(): LinearLayout.LayoutParams =
        LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)

    private fun dp(value: Int): Int = (value * resources.displayMetrics.density + 0.5f).toInt()

    private data class EngineOption(val packageName: String, val label: String)
    private data class LanguageOption(val tag: String, val locale: Locale, val label: String)
    private data class VoiceOption(val voice: Voice?, val label: String)

    companion object {
        private const val ENGINE_TIMEOUT_MS = 7000L

        private fun addLocale(values: MutableMap<String, Locale>, locale: Locale?) {
            if (locale == null || locale.language.isNullOrEmpty()) return
            val tag = normalizedTag(locale)
            if (tag.isNotEmpty() && !values.containsKey(tag)) values[tag] = locale
        }

        private fun normalizedTag(locale: Locale?): String =
            if (locale == null) "" else TtsSettingsStore.normalizeLanguageTag(locale.toLanguageTag())

        private fun languageRank(tag: String): Int = when {
            tag.equals("vi-VN", ignoreCase = true) -> 0
            tag.lowercase(Locale.ROOT).startsWith("vi") -> 1
            else -> 2
        }
    }
}
