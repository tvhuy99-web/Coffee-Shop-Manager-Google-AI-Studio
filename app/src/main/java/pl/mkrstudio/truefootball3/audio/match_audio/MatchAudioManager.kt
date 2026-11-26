package pl.mkrstudio.truefootball3.match_audio

import pl.mkrstudio.truefootball3.audio.*

import android.content.Context
import android.content.res.AssetFileDescriptor
import android.media.AudioAttributes
import android.media.MediaPlayer
import android.media.SoundPool
import android.os.Handler
import android.os.Looper
import java.io.IOException
import java.util.EnumMap
import java.util.Random
import pl.mkrstudio.truefootball3.match_audio.MatchAudioManager.Cue.*

class MatchAudioManager(context: Context, enabled: Boolean) : pl.mkrstudio.truefootball3.presentation.MatchAudioBoundary {
    enum class Cue {
        AMBIENCE_SMALL, AMBIENCE_MEDIUM, AMBIENCE_LARGE,
        CROWD_RISE, PASS_KICK, STRONG_SHOT, HEADER, NET_HIT, POST_CROSSBAR,
        NEAR_MISS, KEEPER_SAVE,
        HOME_GOAL_SMALL, HOME_GOAL_MEDIUM, HOME_GOAL_LARGE,
        AWAY_GOAL_SMALL, AWAY_GOAL_MEDIUM, AWAY_GOAL_LARGE,
        WHISTLE, SUBSTITUTION, POST_MATCH
    }

    private data class LoadedSource(
        val source: SoundSettingsStore.Source,
        val soundId: Int
    )

    private val appContext: Context = context.applicationContext
    private val soundPool: SoundPool
    private val mainHandler = Handler(Looper.getMainLooper())
    private val random = Random()
    private val sources: MutableMap<Cue, MutableList<LoadedSource>> = EnumMap(Cue::class.java)
    private val lastPlayedAt: MutableMap<Cue, Long> = EnumMap(Cue::class.java)
    private val lastIndexes: MutableMap<Cue, Int> = EnumMap(Cue::class.java)
    private val loadedSounds = HashSet<Int>()
    private val activePlayers = ArrayList<MediaPlayer>()
    private val masterVolume: Float = SoundSettingsStore(appContext).volumePercent / 100f
    private var enabled: Boolean = enabled
    private var released: Boolean = false
    private var ambienceRequested: Boolean = false
    private var stadiumCapacity: Int = 30000
    private var ambiencePlayer: MediaPlayer? = null

    init {
        val attrs = AudioAttributes.Builder()
            .setUsage(AudioAttributes.USAGE_GAME)
            .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
            .build()
        soundPool = SoundPool.Builder()
            .setMaxStreams(7)
            .setAudioAttributes(attrs)
            .build()
        soundPool.setOnLoadCompleteListener { _, sampleId, status ->
            if (status == 0) loadedSounds.add(sampleId)
        }
        loadConfiguredLibrary()
    }

    private fun loadConfiguredLibrary() {
        val store = SoundSettingsStore(appContext)
        loadSlot(store, AMBIENCE_SMALL, "stadium_small")
        loadSlot(store, AMBIENCE_MEDIUM, "stadium_medium")
        loadSlot(store, AMBIENCE_LARGE, "stadium_large")
        loadVariants(store, CROWD_RISE, "attack_rise")
        loadVariants(store, PASS_KICK, "pass_kick")
        loadVariants(store, STRONG_SHOT, "strong_shot")
        loadVariants(store, HEADER, "header")
        loadVariants(store, NET_HIT, "net_hit")
        loadVariants(store, POST_CROSSBAR, "post_crossbar")
        loadVariants(store, NEAR_MISS, "near_miss")
        loadVariants(store, KEEPER_SAVE, "keeper_save")
        loadSlot(store, HOME_GOAL_SMALL, "home_goal_small")
        loadSlot(store, HOME_GOAL_MEDIUM, "home_goal_medium")
        loadSlot(store, HOME_GOAL_LARGE, "home_goal_large")
        loadSlot(store, AWAY_GOAL_SMALL, "away_goal_small")
        loadSlot(store, AWAY_GOAL_MEDIUM, "away_goal_medium")
        loadSlot(store, AWAY_GOAL_LARGE, "away_goal_large")
        loadSlot(store, WHISTLE, "referee_whistle")
        loadVariants(store, SUBSTITUTION, "substitution")
        loadVariants(store, POST_MATCH, "post_match")
    }

    private fun loadVariants(store: SoundSettingsStore, cue: Cue, prefix: String) {
        for (i in 1..3) loadSlot(store, cue, "${prefix}_$i")
    }

    private fun loadSlot(store: SoundSettingsStore, cue: Cue, slotId: String) {
        val source = store.resolve(SoundCatalog.find(slotId)) ?: return
        var id = 0
        if (usesSoundPool(cue)) {
            var descriptor: AssetFileDescriptor? = null
            try {
                id = if (source.isAsset) {
                    val assetPath = source.assetPath ?: return
                    descriptor = appContext.assets.openFd(assetPath)
                    soundPool.load(descriptor, 1)
                } else {
                    val file = source.file ?: return
                    soundPool.load(file.absolutePath, 1)
                }
            } catch (_: IOException) {
                return
            } finally {
                try {
                    descriptor?.close()
                } catch (_: IOException) {
                }
            }
        }
        val cueSources = sources.getOrPut(cue) { ArrayList() }
        cueSources.add(LoadedSource(source, id))
    }

    fun setStadiumCapacity(capacity: Int) {
        stadiumCapacity = capacity
    }

    fun setPresentationSeed(seed: Long) {
        random.setSeed(seed)
        lastIndexes.clear()
    }

    fun setEnabled(enabled: Boolean) {
        this.enabled = enabled
        if (!enabled) pauseAll()
    }

    fun startAmbience() {
        if (!enabled || released) return
        ambienceRequested = true
        startAmbienceIfReady()
    }

    private fun startAmbienceIfReady() {
        if (!enabled || released || !ambienceRequested || ambiencePlayer != null) return
        val source = choose(capacityCue("ambience")) ?: chooseFallbackCapacity()
        if (source != null) {
            ambiencePlayer = createMediaPlayer(source.source, 0.62f, looping = true, tracked = false)
        }
    }

    override fun pauseAll() {
        if (released) return
        soundPool.autoPause()
        pausePlayer(ambiencePlayer)
        for (player in ArrayList(activePlayers)) pausePlayer(player)
    }

    override fun resumeAll() {
        if (!enabled || released) return
        soundPool.autoResume()
        resumePlayer(ambiencePlayer)
        for (player in ArrayList(activePlayers)) resumePlayer(player)
        startAmbienceIfReady()
    }

    override fun release() {
        if (released) return
        released = true
        ambienceRequested = false
        mainHandler.removeCallbacksAndMessages(null)
        soundPool.release()
        releasePlayer(ambiencePlayer)
        ambiencePlayer = null
        for (player in ArrayList(activePlayers)) releasePlayer(player)
        activePlayers.clear()
        sources.clear()
        loadedSounds.clear()
    }

    fun play(cue: Cue) {
        play(cue, defaultVolume(cue), cooldownFor(cue))
    }

    private fun play(cue: Cue, volume: Float, cooldownMs: Long) {
        if (!enabled || released) return
        val now = System.currentTimeMillis()
        val last = lastPlayedAt[cue]
        if (last != null && now - last < cooldownMs) return
        val source = choose(cue) ?: return
        if (usesSoundPool(cue)) {
            if (!loadedSounds.contains(source.soundId)) return
            val scaled = scaleVolume(volume)
            soundPool.play(source.soundId, scaled, scaled, 1, 0, 1f)
        } else if (createMediaPlayer(source.source, volume, looping = false, tracked = true) == null) {
            return
        }
        lastPlayedAt[cue] = now
    }

    fun playHomeGoal() {
        play(NET_HIT, 0.95f, 200L)
        playCapacityGoal(true)
    }

    fun playAwayGoal() {
        play(NET_HIT, 0.95f, 200L)
        playCapacityGoal(false)
    }

    private fun playCapacityGoal(home: Boolean) {
        val kind = if (home) "home" else "away"
        var cue = capacityCue(kind)
        if (choose(cue) == null) cue = fallbackCapacityCue(kind)
        play(cue, if (home) 0.98f else 0.90f, 500L)
    }

    fun playPenaltyGoal(home: Boolean) {
        play(STRONG_SHOT, 0.90f, 120L)
        if (home) playHomeGoal() else playAwayGoal()
    }

    fun playPenaltyMiss() {
        play(STRONG_SHOT, 0.90f, 120L)
        play(NEAR_MISS, 0.85f, 250L)
    }

    fun playKickOff() {
        play(WHISTLE, 0.92f, 300L)
        startAmbience()
    }

    fun playHalfTime() {
        playWhistlePattern(2)
    }

    fun playFullTime() {
        playWhistlePattern(3)
        mainHandler.postDelayed({ play(POST_MATCH, 0.90f, 0L) }, 1300L)
    }

    fun duckAmbience(volumePercent: Int, restoreAfterMs: Long = 2200L) {
        if (!enabled || released) return
        setAmbienceVolume(0.62f * volumePercent.coerceIn(20, 100) / 100f)
        mainHandler.postDelayed({ setAmbienceVolume(0.62f) }, restoreAfterMs)
    }

    fun setAmbienceVolume(volume: Float) {
        val player = ambiencePlayer ?: return
        val scaled = scaleVolume(volume)
        try {
            player.setVolume(scaled, scaled)
        } catch (_: IllegalStateException) {
        }
    }

    private fun playWhistlePattern(count: Int) {
        for (i in 0 until count) {
            mainHandler.postDelayed({ play(WHISTLE, 0.92f, 0L) }, i * 720L)
        }
    }

    private fun choose(cue: Cue): LoadedSource? {
        val list = sources[cue]
        if (list.isNullOrEmpty()) return null
        val index = if (list.size == 1) {
            0
        } else {
            val previous = lastIndexes[cue]
            var candidate: Int
            do {
                candidate = random.nextInt(list.size)
            } while (previous != null && candidate == previous)
            candidate
        }
        lastIndexes[cue] = index
        return list[index]
    }

    private fun capacityCue(kind: String): Cue {
        val suffix = SoundCatalog.capacitySuffix(stadiumCapacity)
        return when (kind) {
            "ambience" -> when (suffix) {
                "small" -> AMBIENCE_SMALL
                "large" -> AMBIENCE_LARGE
                else -> AMBIENCE_MEDIUM
            }
            "home" -> when (suffix) {
                "small" -> HOME_GOAL_SMALL
                "large" -> HOME_GOAL_LARGE
                else -> HOME_GOAL_MEDIUM
            }
            else -> when (suffix) {
                "small" -> AWAY_GOAL_SMALL
                "large" -> AWAY_GOAL_LARGE
                else -> AWAY_GOAL_MEDIUM
            }
        }
    }

    private fun fallbackCapacityCue(kind: String): Cue {
        val order = if (kind == "home") {
            arrayOf(HOME_GOAL_MEDIUM, HOME_GOAL_SMALL, HOME_GOAL_LARGE)
        } else {
            arrayOf(AWAY_GOAL_MEDIUM, AWAY_GOAL_SMALL, AWAY_GOAL_LARGE)
        }
        for (cue in order) {
            val list = sources[cue]
            if (!list.isNullOrEmpty()) return cue
        }
        return order[0]
    }

    private fun chooseFallbackCapacity(): LoadedSource? {
        val order = arrayOf(AMBIENCE_MEDIUM, AMBIENCE_SMALL, AMBIENCE_LARGE)
        for (cue in order) {
            val source = choose(cue)
            if (source != null) return source
        }
        return null
    }

    private fun createMediaPlayer(
        source: SoundSettingsStore.Source?,
        volume: Float,
        looping: Boolean,
        tracked: Boolean
    ): MediaPlayer? {
        if (released || source == null) return null
        if (tracked && activePlayers.size >= 4) releasePlayer(activePlayers.removeAt(0))
        var descriptor: AssetFileDescriptor? = null
        var created: MediaPlayer? = null
        try {
            val player = MediaPlayer()
            created = player
            player.setAudioAttributes(
                AudioAttributes.Builder()
                    .setUsage(AudioAttributes.USAGE_GAME)
                    .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
                    .build()
            )
            if (source.isAsset) {
                val assetPath = source.assetPath ?: return null
                descriptor = appContext.assets.openFd(assetPath)
                player.setDataSource(descriptor.fileDescriptor, descriptor.startOffset, descriptor.length)
            } else {
                val file = source.file ?: return null
                player.setDataSource(file.absolutePath)
            }
            player.isLooping = looping
            val scaled = scaleVolume(volume)
            player.setVolume(scaled, scaled)
            if (tracked) {
                player.setOnCompletionListener { completed ->
                    activePlayers.remove(completed)
                    releasePlayer(completed)
                }
                player.setOnErrorListener { failed, _, _ ->
                    activePlayers.remove(failed)
                    releasePlayer(failed)
                    true
                }
                activePlayers.add(player)
            }
            player.prepare()
            player.start()
            return player
        } catch (_: IOException) {
            activePlayers.remove(created)
            releasePlayer(created)
            return null
        } catch (_: IllegalStateException) {
            activePlayers.remove(created)
            releasePlayer(created)
            return null
        } finally {
            try {
                descriptor?.close()
            } catch (_: IOException) {
            }
        }
    }

    private fun scaleVolume(volume: Float): Float = (volume * masterVolume).coerceIn(0f, 1f)

    companion object {
        private fun usesSoundPool(cue: Cue): Boolean = when (cue) {
            PASS_KICK, STRONG_SHOT, HEADER, NET_HIT, POST_CROSSBAR, WHISTLE -> true
            else -> false
        }

        private fun defaultVolume(cue: Cue): Float = when (cue) {
            PASS_KICK -> 0.78f
            STRONG_SHOT, HEADER -> 0.90f
            NET_HIT, WHISTLE -> 0.92f
            CROWD_RISE -> 0.82f
            else -> 0.85f
        }

        private fun cooldownFor(cue: Cue): Long = when (cue) {
            PASS_KICK, STRONG_SHOT, HEADER -> 220L
            CROWD_RISE -> 450L
            WHISTLE -> 300L
            else -> 150L
        }

        private fun pausePlayer(player: MediaPlayer?) {
            if (player == null) return
            try {
                if (player.isPlaying) player.pause()
            } catch (_: IllegalStateException) {
            }
        }

        private fun resumePlayer(player: MediaPlayer?) {
            if (player == null) return
            try {
                if (!player.isPlaying) player.start()
            } catch (_: IllegalStateException) {
            }
        }

        private fun releasePlayer(player: MediaPlayer?) {
            if (player == null) return
            try {
                player.reset()
            } catch (_: IllegalStateException) {
            }
            player.release()
        }
    }
}
