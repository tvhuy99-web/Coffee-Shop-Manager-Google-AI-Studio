# A6 presentation extraction
14 production Kotlin files grouped physically into match_playback, match_commentary and match_audio. Existing Kotlin package namespaces intentionally preserved in this first slice. Gradle commentary token validation and JVM runners/static checks updated together. No lookahead/rate policy or targetSdk change. Explicit capabilities and Activity binder extraction remain open; no A6 closure claimed.

Actual controller, scheduler and audio manager now implement PresentationBoundary capabilities. Kotlin package renaming and Activity binder extraction remain open; compatibility namespaces intentionally retained.

MatchEventCueAdapter owns event-to-cue conversion and its score/minute projection. Activity delegates; existing fallback text, filtered goal accounting and renderer inputs retained. New JVM test exercises own goals, score context and added time. Full Activity binder extraction and namespace separation still outstanding.
