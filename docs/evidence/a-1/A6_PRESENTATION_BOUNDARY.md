# A6 presentation package migration

Stacked on A3–A5 (candidate 5fc57f3). The old ABI and JNI package remain unchanged. Actual Kotlin production implementations have moved, rather than adding interfaces with no callers.

- match_playback: MatchPlaybackController, LabV4PlaybackCoordinator, LabRustLiveSession, MatchCheckpointStore.
- match_commentary: LabCommentaryEngine, LabCommentaryTemplatePolicy, LabNarrativeTracker, NarrationScheduler.
- match_audio: LabAudioDirector. Android platform SFX/TTS managers remain under audio as platform services.
- lab: activity, existing DTOs, NativeBridge, input factory, settings. Existing playback controller remains the worker/clock/session owner; activity still binds events, UI, dialogs, archive and presentation conversion.
- JVM scripts and static checks follow the production package paths. Commentary token task reads relocated renderer. Test doubles are limited to external session/audio platform dependencies, not the production controller, narration scheduler or highlight filter.
- No lookahead/rate/commentary policy changes, no targetSdk changes, no gameplay edits.
- Further extraction of activity-side event conversion, explicit session identity validation, full A7 acceptance, and physical Android A8 testing are outstanding. Do not mark release accepted until A7 and A8 gates have evidence.
