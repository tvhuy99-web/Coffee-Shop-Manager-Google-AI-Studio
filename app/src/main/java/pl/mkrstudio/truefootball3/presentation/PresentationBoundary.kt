package pl.mkrstudio.truefootball3.presentation

/** Presentation capabilities. No Rust world-state mutation or RNG access. */
interface MatchPlaybackBoundary {
    fun pause()
    fun resume()
    fun setPlaybackRate(newRate: Float)
    fun submitCommandJson(commandJson: String, onResult: (ok: Boolean, responseJson: String) -> Unit)
    fun release()
}
interface MatchCommentaryBoundary {
    fun pause()
    fun resume()
    fun setPlaybackRate(rate: Float)
    fun onClockTick(committedSimTimeMs: Long)
    fun release()
}
interface MatchAudioBoundary {
    fun pauseAll()
    fun resumeAll()
    fun release()
}
