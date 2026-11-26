# A6 presentation extraction
14 production Kotlin files grouped physically into match_playback, match_commentary and match_audio. Existing Kotlin package namespaces intentionally preserved in this first slice. Gradle commentary token validation and JVM runners/static checks updated together. No lookahead/rate policy or targetSdk change. Explicit capabilities and Activity binder extraction remain open; no A6 closure claimed.

Actual controller, scheduler and audio manager now implement PresentationBoundary capabilities. Kotlin package renaming and Activity binder extraction remain open; compatibility namespaces intentionally retained.
