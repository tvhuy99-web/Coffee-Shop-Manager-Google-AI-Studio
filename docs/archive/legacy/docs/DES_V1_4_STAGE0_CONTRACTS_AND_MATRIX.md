# MATCH ENGINE V4 — HỒ SƠ KHÓA ĐẶC TẢ KỸ THUẬT & MA TRẬN NGHIỆM THU (GIAI ĐOẠN 0 — DES v1.4)

- **Dự án:** Football Match Engine V4 (`tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio`)
- **Nhánh phát triển cục bộ:** `feat/des-v1.4` (khởi tạo từ snapshot `main` trong gói kho mã nguồn)
- **Tài liệu chuẩn:** `Match_Engine_V4_DES_v1.4_Ke_hoach_chot_kien_truc.txt`
- **Ngày khóa Giai đoạn 0:** 24/11/2026
- **Trạng thái tổng thể:**
  - **Giai đoạn 0 (Khóa đặc tả kỹ thuật):** `ACCEPTED` (đã đối chiếu toàn bộ mã nguồn thực tế bởi 4 nhóm tác nhân độc lập + kiểm tra tích hợp & QA).
  - **Giai đoạn 1–8 (Đo baseline, triển khai & nghiệm thu thực thi):** `BLOCKED` do giới hạn môi trường hiện tại (thiếu `rustc`/`cargo`, `javac`/`kotlinc`/`gradle`/Android SDK & NDK, không có internet để tải PR #5 và crate/dependency).

---

## 1. Xác nhận Năng lực Môi trường & Tác nhân thực tế

### 1.1. Cơ cấu Đa tác nhân thực tế đã triển khai
Nền tảng hỗ trợ khởi tạo tác nhân con độc lập thực sự (`invoke_subagent`). Kỹ sư trưởng (Agent 0) đã thành lập và điều phối song song 4 nhóm tác nhân chuyên môn cho Giai đoạn 0:
1. **Nhóm Agent 1 & 2 (Rust DES, Time & Football Physics):** Đọc toàn bộ `clock.rs`, `v4/mod.rs`, `v4/events.rs`, `v4/session.rs`, `v4/movement.rs`, `v4/perception.rs`, `v4/ball_play.rs`, `v4/shooting.rs`, `v4/spatial.rs`.
2. **Nhóm Agent 3 & 4 (Rules, DeadBall, Fitness, Tactics & Calibration):** Đọc toàn bộ `v4/set_pieces.rs`, `v4/state.rs`, `v4/psychology.rs`, `v4/tactics.rs`, `v4/calibration.rs`, `v4/ai_manager.rs`, `v4/input.rs`, `v4/stats.rs`.
3. **Nhóm Agent 5 & 6 (Android, JNI, Checkpoint, Playback & TTS):** Đọc toàn bộ `jni_bridge.rs`, `NativeBridge.kt`, `LabRustLiveSession.kt`, `LabGlobalSeedReplayStore.kt`, `LabReferenceMatchActivity.kt`, `LabV4PlaybackCoordinator.kt`, `LabAudioDirector.kt`, `LabModels.kt`, `MatchTtsManager.kt`, `TtsSettingsStore.kt`, `app/build.gradle`.
4. **Nhóm Agent 7 & 8 (Independent QA & Integration Review):** Đọc `PROJECT_ARCHITECTURE_AND_RULES.md`, `docs/MATCH_ENGINE_V4_P2.md`, `v4/tests.rs` (1.409 dòng, 27 test), `wire_json.rs`, `tests/`, `tools/`, và chạy xác minh các script Python baseline hiện có.

### 1.2. Hiện trạng Công cụ & Trở ngại Môi trường (Bằng chứng thực tế)

| Thành phần | Trạng thái trên VM | Ảnh hưởng trực tiếp |
| :--- | :--- | :--- |
| `python3` (3.11.2), `git` (2.39.5), `java` (OpenJDK 17 JRE) | **Sẵn sàng** | Đã chạy `verify_v4_p0_p1_patch.py` (12/12 PASS), `verify_v4_p1_234.py` (22/22 PASS), `verify_v4_p2.py` (PASS), `verify_parity_fixtures.py` (PASS, 3 trận / 7.044 sự kiện). |
| `rustc` / `cargo` / Cargo vendor cache | **KHÔNG CÓ** | Không thể chạy `cargo check`, `cargo test --all-targets`, hay benchmark Monte Carlo trong `calibration.rs`. |
| `javac` / `kotlinc` / `gradle` / Android SDK & NDK / `adb` | **KHÔNG CÓ** | Không thể chạy `tests/p1_kotlin/Smoke.kt`, `tests/commentary/CommentaryRegression.kt`, biên dịch `.so` ARM64/x86_64 hay build/test APK trên thiết bị Android thật. |
| Truy cập mạng / GitHub PR #5 (V4.1 Diagnostics) | **BỊ TẮT (Offline)** | Đã xác minh `v4/calibration.rs` trong bản nén `main` **chưa có PR #5** (chỉ có 16 trường tổng hợp cơ bản trong `MonteCarloReportV4`). Không thể tự fetch PR #5 hay tải `androidx.media3`. |

---

## 2. Kết quả Đối chiếu Mã nguồn Thực tế với DES v1.4

Toàn bộ các nhận định kiến trúc trong DES v1.4 đều đã được đối chiếu tới từng dòng mã nguồn và **khớp 100%**. Ngoài ra, đội ngũ phát hiện thêm **6 vấn đề thực tế chưa được liệt kê trong DES v1.4** bắt buộc phải đưa vào phạm vi triển khai:

1. **Công thức phụ thuộc nhịp thứ 6 (`v4/ball_play.rs:537-550`):** `execute_dribble_duel` gọi cứng `fatigue.apply_interval(2.0, 11.0/10.0, true, ...)` mỗi lượt gọi — cần chuyển đổi cùng 5 công thức ở Mục IX.1.
2. **Bộ đếm áp lực phụ thuộc nhịp (`v4/state.rs:194`, `v4/tactics.rs:70-76, 105`):** `sustained_pressure_streak: u8` tăng `+1` mỗi nhịp và kích hoạt cờ ở `>= 3` — dưới DES (bước ~1–2s), cờ sẽ bật chỉ sau 3 giây thay vì ~30 giây; cần đổi sang `sustained_pressure_ms: i32`.
3. **Nhiễu mục tiêu di chuyển theo lượt gọi (`v4/movement.rs:207-208, 232-237`):** `jitter_x/y` và `lapse_x/y` phát sinh trên mỗi lần gọi `advance_off_ball_movement`, sẽ khiến cầu thủ giật cục khi `dt_sec < 1s`; cần tách cập nhật `target_pos` khỏi bước tích phân liên tục.
4. **Duyệt tuyến tính `String` ngoài `lookup_attrs`:** `slot_anchor_m` (`v4/movement.rs:294-309`), `weak_foot_pass_modifier` (`v4/ball_play.rs:707-742`) và `weak_foot_shot_modifier` (`v4/shooting.rs:593-621`) cũng duyệt tuyến tính theo `String` và cần chuyển sang truy cập $O(1)$ qua `profile_idx` / `slot_idx`.
5. **Thiếu `OWN_GOAL` ở chế độ Highlights ngắn (`LabV4PlaybackCoordinator.kt:75`):** Bộ lọc `SHORT_HIGHLIGHTS` đang bỏ sót `OWN_GOAL`; cần bổ sung khi xây `ViewingModeTimelineFilter`.
6. **Sai lệch thứ tự gọi trong `v4/session.rs`:** `update_interval_fatigue` (dòng `429`) đang được gọi *trước* `advance_off_ball_movement` (dòng `446`), cần đảo thứ tự để lấy `moved_m` thực tế từ `movement.rs`.

---

## 3. Khóa 4 Hợp đồng Nền tảng & Giao diện Kỹ thuật (DES v1.4)

### 3.1. Đồng hồ mili-giây (`clock.rs`) & Phiên bản giao thức (`v4/mod.rs`, `v4/events.rs`)
- **Kiểu dữ liệu thời gian nội bộ:** Toàn bộ dùng `i64` mili-giây (`sim_time_ms`, `period_elapsed_ms`, `ball_in_play_ms`, `ball_out_of_play_ms`, `compensable_lost_ms`, `announced_added_ms`, `additional_added_ms`).
- **Hàm đọc bù giờ động:** `effective_added_ms() = announced_added_ms + additional_added_ms`. Xóa bỏ `added_target` khóa tĩnh và trần bù giờ nghiệp vụ cứng (`added_time_cap_seconds` 240s/360s/120s); thay bằng hàng rào kỹ thuật riêng (`watchdog_max_period_ms`) chỉ để báo lỗi chẩn đoán vòng lặp vô hạn.
- **Khóa phiên bản:** Nâng đồng bộ `JNI_BRIDGE_VERSION: 7 -> 8`, `ENGINE_PROTOCOL_VERSION_V4: 4 -> 5`, `EVENT_SCHEMA_VERSION_V4: 4 -> 5`, `ENGINE_VERSION_V4 = "v4.1.0-des"`.
- **Tương thích ngược trường hiển thị:** `MatchEventV4` và `SessionStepResultV4` thêm `sim_time_ms: i64`, `period_elapsed_ms: i64`, `stage: SessionStageV4`, giữ `game_second = (sim_time_ms / 1000) as i32` và `period_elapsed_second` làm trường dẫn xuất.

### 3.2. `EventScheduler` & Hợp đồng A — Tương tác liên tục (`v4/scheduler.rs`, `v4/interaction.rs`)
- **Hàng đợi ưu tiên tất định:** `BinaryHeap` sắp theo `(event_time_ms, event_priority, insertion_sequence)`.
- **Thứ tự ưu tiên đồng mốc `ms` (`EventPriority`):**
  1. `PhysicsAndBoundary (10)`: cắt bóng, cản phá, vạch biên, vạch cầu môn.
  2. `RuleAndOfficiating (20)`: bàn thắng, phạm lỗi, việt vị, thẻ phạt, chấn thương.
  3. `BallStateTransition (30)`: đỡ bước một, chuyển kiểm soát, tranh chấp bóng hai.
  4. `DeadBallAndSubstitution (40)`: mở `DeadBallState`, xả `pending_substitutions`.
  5. `TacticalAndCommand (50)`: áp dụng lệnh chiến thuật đến mốc hiệu lực, AI manager.
  6. `ActionDecision (60)`: quan sát/ra quyết định, thực hiện chuyền/sút/đá phạt.
  7. `PeriodBoundary (70)`: công bố bù giờ, còi kết thúc hiệp.
- **Vô hiệu hóa & Chống lặp vô hạn:** Dùng `live_play_generation: u64` và `period_generation: u64` để hủy sự kiện cũ; từ chối lịch ở quá khứ (`event_time_ms < current_time_ms`); chặn `zero_dt_streak > 64` và `total_popped > 250_000`.
- **Phát hiện tương tác giữa hai mốc DES (2 tầng):**
  - Tầng 1 (Giải tích): Nghiệm sớm nhất của `BallTrajectory` với đường biên/khung thành và `closest_approach` với hậu vệ có xét tốc độ chạy + độ trễ phản ứng.
  - Tầng 2 (Adaptive Substep giới hạn): Chỉ bật trong vùng rủi ro giao cắt (`50..=200ms`, tối đa `16` substep).
  - **Bất biến tích lũy đơn:** `find_earliest_interaction` thuần đọc; chỉ gọi `advance_off_ball_movement(dt_sec)` **đúng 1 lần** cho khoảng `[t_now, resolved_time_ms]` rồi cấp `moved_m` thực tế cho `TwoPoolFatigueState`.

### 3.3. Hợp đồng D — `DeadBallState`, `RulesetConfig` & `TimingModel` có phiên bản
- **Trạng thái hiệp (`HalfStageV4`):** Bổ sung `HalfTimeBreak` và `ExtraTimeBreak { next_period }`. Phiên tương tác dừng tại đây và tiếp tục qua `MatchCommandV4::StartNextPeriod`; `FastBatch` tự động hồi phục và đi tiếp.
- **Máy trạng thái `DeadBallState`:** Tách biệt hoàn toàn khỏi bóng sống (`GoalCelebration` → `ReturningToHalf` → `KickoffSetup`, `ThrowIn`, `Corner`, `GoalKick`, `DirectFreeKick`, `IndirectFreeKick` (bao gồm `Offside`), `PenaltyKick`, `InjuryStoppage`, `SubstitutionWindow`). Cấm tuyệt đối hành động bóng sống khi `dead_ball.is_some()`.
- **Tách `RulesetConfig` khỏi `TimingModelConfig`:** `RulesetIdentity { ruleset_id, ruleset_version, timing_model_id, timing_model_version }` bất biến từ khai cuộc, đưa vào `input_fingerprint` và `MatchCheckpoint`.
- **Neo lệnh mới (`AppliedCommandV4`):** Bỏ `completed_beats`, dùng `accepted_sim_time_ms`, `scheduled_effective_sim_time_ms`, `actual_effective_sim_time_ms`, `applied_step_index`, `status`. Thay người bóng sống vào `pending_substitutions` chờ bóng chết; lệnh chiến thuật có độ trễ tự nhiên quy mô vài phút mô phỏng.

### 3.4. Hợp đồng C — JNI mới & `MatchCheckpoint` (`jni_bridge.rs`, `NativeBridge.kt`, `MatchCheckpointStore`)
- **4 hàm JNI mới:**
  - `nativeAdvanceV4SessionUntilMs(handle: Long, targetSimTimeMs: Long, maxStepsGuard: Int): String`
  - `nativeStartNextPeriod(handle: Long): String`
  - `nativeExportV4Checkpoint(handle: Long): String`
  - `nativeImportV4Checkpoint(checkpointJson: String): String`
- **Tách biệt lưu trữ:** Giữ nguyên `LabGlobalSeedReplayStore` (immutable, cho trận `FULL_TIME`). Tạo mới `MatchCheckpointStore` (`filesDir/v4_active_checkpoints/`, mutable, ghi `AtomicFile` + `fd.sync()`, checksum 2 lớp SHA-256 + CRC32).
- **Dual-Frontier chống nhiễm Speculative Lookahead:** Chỉ lưu `lastCommittedRustCheckpointJson` tại mốc đã xác nhận (không lưu nhánh speculative lookahead, không lưu đường dẫn WAV cache, không lưu `elapsedRealtime()` cũ). Khi app vào nền tự `pause`, mở lại khởi động `paused = true` và khôi phục idempotent (không lặp bàn thắng/tỷ số/thay người).

### 3.5. Hợp đồng B — `MatchPlaybackController` & `NarrationScheduler` 1.0x–3.0x
- **Đồng hồ phát lại duy nhất:** Hợp nhất `LabReferenceMatchActivity` và `LabV4PlaybackCoordinator` về `MatchPlaybackController` dùng `SystemClock.elapsedRealtime()` với công thức re-anchor không trôi khi đổi tốc độ `1.0x / 1.25x / 1.5x / 2.0x / 3.0x`.
- **Đồng bộ `LabAudioDirector`:** Chuyển toàn bộ `HYSTERESIS_MS`, `DECAY_MS`, `CROWD_RISE_COOLDOWN_MS`, `allowCooldown()` sang trục `sim_time_ms`.
- **Lựa chọn trình phát giữ cao độ (chốt ở Giai đoạn 0):** Chọn phương án tích hợp sẵn **`android.media.MediaPlayer` / `AudioTrack` kèm `PlaybackParams().setSpeed(rate).setPitch(1.0f)`** (hỗ trợ sẵn từ API 23, khớp `minSdk 26` trong `app/build.gradle` và không cần thêm dependency `androidx.media3` vào `libs/app-dependencies.jar`).
- **Pipeline TTS chuẩn 1.0x:** Tách `SimulationEvent` vs `CommentaryCue` (`cueId`, `eventIds`, `text`, `anchorSimTimeMs`, `durationAudioMs`, `generationToken`). Chốt danh sách cue khả thi ở 1.0x sau khi đo thời lượng thực từ `TextToSpeech.synthesizeToFile()` (mục tiêu khởi đầu ~4,2 âm tiết/giây tiếng Việt); giữ nguyên 100% nội dung và thứ tự từ `1.0x` đến `3.0x` (ở `3.0x` chỉ hiện cảnh báo khó nghe, tuyệt đối không tắt TTS). Lookahead Buffer 10–20s + Ring-buffer WAV tối đa 20–30 file.
- **Chính sách backlog TTS:** Nếu phần cứng tổng hợp/phát trễ quá ngưỡng hoặc lỗi backend, giữ nguyên đồng hồ trận và kết quả Rust, ghi nhận trạng thái suy giảm âm thanh minh bạch (bỏ cơ chế tắt ngầm `consecutive >= 3` của `MatchTtsManager.kt:320-329`), tuyệt đối không lén cắt câu vì người chơi tăng tốc.

---

## 4. Ma trận Yêu cầu DES v1.4 — Công việc — Tác nhân — Kiểm thử & Nghiệm thu

| Mã CV | Giai đoạn | Hạng mục DES v1.4 | Tệp / Module chính | Phụ thuộc | Thực hiện | Kiểm tra độc lập | Bài kiểm thử bắt buộc | Trạng thái |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **T0.1** | GĐ 0 | Khóa đặc tả 4 hợp đồng A–D, schema JNI/Checkpoint, Playback/TTS & ma trận QA | `docs/DES_V1_4_STAGE0_CONTRACTS_AND_MATRIX.md` | — | Agent 0–6 | Agent 7 & 8 | Đối chiếu 100% mã nguồn + chạy `verify_v4_*.py` | **ACCEPTED** |
| **T1.1** | GĐ 1 | Tích hợp PR #5 (V4.1 Diagnostics) & mở rộng `calibration.rs`, chạy baseline | `v4/calibration.rs`, `v4/stats.rs` | T0.1 + PR #5 + `cargo` | Agent 4 | Agent 7 & 8 | `cargo test` + Monte Carlo baseline report | **BLOCKED** |
| **T2.1** | GĐ 2 | `ClockState` ms (`i64`), bỏ `added_cap`, đọc động `effective_added_ms()`, `HalfTimeBreak` | `clock.rs`, `v4/mod.rs`, `v4/events.rs`, `v4/session.rs` | T1.1 | Agent 1 | Agent 7 & 8 | `des_clock_dynamic_added_time_no_hard_cap`, `des_halftime_break_*` | **PENDING (Blocked by T1.1)** |
| **T2.2** | GĐ 2 | `EventScheduler` (priority/seq/gen, hủy sự kiện, chống lặp) & `ContinuousInteraction` | `v4/scheduler.rs`, `v4/interaction.rs`, `v4/spatial.rs` | T2.1 | Agent 1 & 2 | Agent 7 & 8 | `des_scheduler_*`, `des_ci_*` (6 edge-cases) | **PENDING** |
| **T2.3** | GĐ 2 | `advance_until_ms`, `export/import_checkpoint`, tối ưu `profile_idx`/`slot_idx` | `v4/session.rs`, `jni_bridge.rs`, `v4/movement.rs`, `v4/state.rs` | T2.2 | Agent 1 & 5 | Agent 7 & 8 | `des_checkpoint_roundtrip_*`, `des_fullevent_fastbatch_exact_parity_*` | **PENDING** |
| **T3.1** | GĐ 3 | Máy trạng thái `DeadBallState`, `RulesetConfig` phiên bản, `TimingModel`, `pending_substitutions` | `v4/set_pieces.rs`, `v4/state.rs`, `v4/session.rs`, `v4/rules.rs` | T2.3 | Agent 3 | Agent 7 & 8 | `des_goal_celebration_then_kickoff_*`, `des_deadball_*`, `des_live_sub_waits_*` | **PENDING** |
| **T4.1** | GĐ 4 | Hành động bóng sống DES (`InFlight`, 1 chạm, khống chế, rê, sút), bỏ `clamp(1.0, 6.0)` | `v4/ball_play.rs`, `v4/perception.rs`, `v4/shooting.rs`, `v4/movement.rs` | T3.1 | Agent 2 | Agent 7 & 8 | `des_ci_defender_intercepts_before_receiver`, chuỗi phối hợp 1 chạm | **PENDING** |
| **T5.1** | GĐ 5 | Chuyển đổi 6 công thức phụ thuộc nhịp + `sustained_pressure_ms`, độ trễ chiến thuật, hiệu chỉnh Monte Carlo | `v4/state.rs`, `v4/psychology.rs`, `v4/tactics.rs`, `v4/session.rs`, `v4/ball_play.rs`, `v4/calibration.rs` | T4.1 | Agent 4 | Agent 7 & 8 | `des_tactical_command_delayed_*`, bất biến chia bước thể lực, Monte Carlo | **PENDING** |
| **T6.1** | GĐ 6 | `MatchPlaybackController` hợp nhất, `LabAudioDirector` theo `simTimeMs`, `MatchCheckpointStore` | `LabReferenceMatchActivity.kt`, `LabV4PlaybackCoordinator.kt`, `LabAudioDirector.kt`, `LabRustLiveSession.kt`, `NativeBridge.kt` | T2.3, T3.1 | Agent 5 & 6 | Agent 7 & 8 | `kt_playback_rate_invariance_*`, `kt_audio_director_uses_sim_time_*`, `kt_checkpoint_*` | **PENDING** |
| **T7.1** | GĐ 7 | `NarrationScheduler` 1.0x, `synthesizeToFile()`, Lookahead 10–20s, `PlaybackParams` 1.0x–3.0x, ring-buffer WAV | `MatchTtsManager.kt`, `NarrationScheduler.kt`, `TtsSettingsStore.kt` | T6.1 | Agent 6 | Agent 7 & 8 | `kt_narration_scheduler_*`, `kt_tts_failure_or_backlog_*`, `android_wav_ring_buffer_*` | **PENDING** |
| **T8.1** | GĐ 8 | Kiểm thử tổng thể: Monte Carlo, Replay tương thích ngược, Kill-Process 5 trạng thái, thiết bị Android ARM64 | Toàn bộ `rust/`, `app/`, `tests/`, `tools/` | T5.1, T7.1 | Agent 7 & 8 | Agent 0 | Full `cargo test`, JVM Smoke, Commentary Regression, Android APK/Device | **PENDING** |
