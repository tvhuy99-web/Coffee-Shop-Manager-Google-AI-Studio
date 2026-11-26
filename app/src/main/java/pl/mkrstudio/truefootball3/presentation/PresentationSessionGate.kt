package pl.mkrstudio.truefootball3.presentation

/** A callback belongs to exactly one Activity-owned match generation.
 * The token is immutable and published atomically to the worker thread.
 */
class PresentationSessionGate {
    data class Token internal constructor(val matchId: String, val generation: Long)
    private var nextGeneration = 0L
    @Volatile private var current: Token? = null

    @Synchronized fun begin(matchId: String): Token {
        check(matchId.isNotBlank())
        val token = Token(matchId, ++nextGeneration)
        current = token
        return token
    }

    fun isCurrent(token: Token): Boolean = current == token

    @Synchronized fun invalidate() { current = null }
}
