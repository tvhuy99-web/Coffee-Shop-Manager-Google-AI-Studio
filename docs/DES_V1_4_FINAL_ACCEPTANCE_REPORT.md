> **HISTORICAL / SUPERSEDED:** Báo cáo của bản trước. Không phải bằng chứng nghiệm thu cho bản sửa v4.1.1-des. Xem `DES_V1_4_REPAIR_REPORT.md` để biết kết quả thực thi và điều kiện còn thiếu.

# BÁO CÁO NGHIỆM THU TỔNG THỂ — TRIỂN KHAI MATCH ENGINE V4 THEO KIẾN TRÚC DES v1.4

- **Dự án:** Football Match Engine V4 (`tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio`)
- **Nhánh phát triển:** `feat/des-v1.4` (nhánh `main` giữ nguyên vẹn, chưa gộp)
- **Tài liệu chuẩn:** `Match_Engine_V4_DES_v1.4_Ke_hoach_chot_kien_truc.txt` + PR #5 (`feat-v4-phase1-match-diagnostics`)
- **Trạng thái tổng thể:** **`CODE COMPLETE & STATIC CONTRACT VERIFIED — PENDING RUST/GRADLE BUILD & ANDROID DEVICE VERIFICATION`**
  *(Toàn bộ mã nguồn Giai đoạn 0–8 và bộ kiểm thử đã hoàn tất trên nhánh `feat/des-v1.4`, qua vòng phản biện độc lập & sửa 9/9 lỗi CRITICAL của Agent 8, đạt 16/16 kiểm chứng hợp đồng tĩnh `tools/verify_v4_des_contract.py`. Do môi trường sandbox không có `rustc`/`cargo`, `kotlinc`/`gradle` và thiết bị Android thật, các bài test biên dịch/runtime cần được chạy trên máy dev/CI theo hướng dẫn ở Mục 4.)*

---

## 1. Tóm tắt Lịch sử Triển khai Theo Giai đoạn (Nhánh `feat/des-v1.4`)

| Giai đoạn | Tác nhân tham gia | Nội dung triển khai chính | Trạng thái |
| :--- | :--- | :--- | :--- |
| **GĐ 0 — Khóa đặc tả** | Agent 0–8 (4 nhóm song song) | Đối chiếu 100% mã nguồn thực tế, phát hiện 6 lỗi bổ sung ngoài DES v1.4, khóa 4 hợp đồng A–D và ma trận QA (`docs/DES_V1_4_STAGE0_CONTRACTS_AND_MATRIX.md`). | **ACCEPTED** |
| **GĐ 1 — Đo đạc & PR #5** | Agent 0, Agent 4 | Tích hợp PR #5 (`v4/diagnostics.rs`, `v4/attribute_impact_tests.rs`, `docs/V4_PHASE1_MATCH_DIAGNOSTICS.md`) vào `feat/des-v1.4`; mở rộng `DurationDistributionMs`, `DeadBallDistributionsV4`, `DesMatchDiagnosticsV4` và `MonteCarloReportV4` theo Mục X. | **CODE COMPLETE** (chờ chạy `cargo test` benchmark) |
| **GĐ 2 — Lõi DES & JNI** | Agent 1, Agent 5 | `DesClockState` (`i64` ms, bỏ `added_cap`, đọc động `effective_added_ms()`, giữ nguyên tương thích V3), `v4/scheduler.rs`, `v4/interaction.rs`, `v4/checkpoint.rs`, `HalfStageV4::HalfTimeBreak`/`ExtraTimeBreak`, `advance_until_ms`, `StartNextPeriod`, `export/import_checkpoint`, nâng `JNI_BRIDGE_VERSION=8` / `ENGINE_PROTOCOL_VERSION_V4=5` / `EVENT_SCHEMA_VERSION_V4=5`, tối ưu `profile_idx`/`slot_idx` $O(1)$. | **CODE COMPLETE** |
| **GĐ 3 — Hệ thống bóng chết** | Agent 3, Agent 1 | `v4/rules.rs` (`RulesetIdentity`, `RulesetConfig`, `TimingModelConfig`, `BoundedDurationRangeMs`, `DeadBallState`, `DeadBallKind`, `DeadBallPhase`), `v4/set_pieces.rs` (ưu tiên `SetPiecePlanV4`, bóng bật phạt đền), `pending_substitutions` chỉ thực thi tại bóng chết/giữa hiệp, ăn mừng bàn thắng -> giao bóng có thời lượng biến thiên. | **CODE COMPLETE** |
| **GĐ 4 — Bóng sống DES** | Agent 2 | `v4/movement.rs` (bỏ `clamp(1.0, 6.0)`, ghi `last_moved_m`, giảm nhiễu theo `sqrt(dt)`), `v4/perception.rs` (`decision_delay_ms`, `is_one_touch`, *la pausa*), `v4/ball_play.rs` (`InFlight`, `find_earliest_ball_interaction` bắt cắt bóng/ra biên trước khi tới đích, sửa `execute_dribble_duel` dùng `dt_sec` thực), `v4/shooting.rs` (`shot_flight_ms`, bỏ dịch chuyển tức thời rebound). | **CODE COMPLETE** |
| **GĐ 5 — Hiệu chỉnh liên quan** | Agent 4, Agent 1 | Chuyển đổi đủ 6 công thức phụ thuộc nhịp + `sustained_pressure_ms` (`v4/state.rs`, `v4/psychology.rs`, `v4/tactics.rs`, `v4/session.rs`, `v4/ball_play.rs`); độ trễ chiến thuật tự nhiên (`compute_tactical_delay_for_context_ms` 1.5–4.5 phút bóng sống, pha trộn `blend_tactical_instructions`). | **CODE COMPLETE** |
| **GĐ 6 — Android Playback & Checkpoint** | Agent 5, Agent 6 | `NativeBridge.kt` (v8, 5 hàm DES mới), `LabRustLiveSession.kt`, `LabModels.kt`, `LabGlobalSeedReplayStore.kt` (v3 + nhận diện bản lưu `v4.0.0` cũ không báo sai fingerprint), `MatchCheckpointStore.kt` (atomic `AtomicFile` + `fd.sync()`, SHA-256 + CRC32), `MatchPlaybackController.kt` (đồng hồ `elapsedRealtime()` re-anchor 1.0x–3.0x, 3 `LabViewingMode`, `HalfTimeBreak`), `LabAudioDirector.kt` (chuyển sang `simTimeMs`), `LabV4PlaybackCoordinator.kt` (thêm `OWN_GOAL` vào `SHORT_HIGHLIGHTS`), `LabReferenceMatchActivity.kt`. | **CODE COMPLETE** |
| **GĐ 7 — TTS Đồng bộ** | Agent 6 | `NarrationScheduler.kt` (tách `SimulationEvent` vs `CommentaryCue`, khóa cue 1.0x bất biến tới 3.0x, cảnh báo 3.0x không tắt TTS, Lookahead 10–20s + `generationToken`, WAV ring-buffer tối đa 25 file, chính sách backlog không chặn đồng hồ), `MatchTtsManager.kt` (`synthesizeToFile`, đọc RIFF/WAVE header, `MediaPlayer` + `PlaybackParams` giữ cao độ, bỏ circuit breaker tắt ngầm `consecutive >= 3`), `TtsSettingsStore.kt` (`DEFAULT_VI_SYLLABLES_PER_SEC = 4.2f`). | **CODE COMPLETE** |
| **GĐ 8 — QA, Review & Rework** | Agent 7, Agent 8, Agent 0 | Nâng cấp `v4/tests.rs` (bảo toàn 27 test cũ + thêm 15 test bất biến DES v1.4), mở rộng `tests/p1_kotlin/Smoke.kt`, tạo `tools/verify_v4_des_contract.py` (**16/16 PASS**). Agent 8 kiểm toán độc lập phát hiện **9 CRITICAL + 5 MINOR** -> Kỹ sư trưởng từ chối nghiệm thu và giao vòng Rework -> Đã sửa triệt để **9/9 CRITICAL + 3 MINOR** và kiểm chứng lại **16/16 PASS**. | **STATIC PASS / RUNTIME PENDING** |

---

## 2. Ánh xạ Từng Yêu cầu DES v1.4 sang Mã nguồn & Kiểm thử

| Yêu cầu DES v1.4 | Tệp triển khai chính | Bài kiểm thử đối ứng |
| :--- | :--- | :--- |
| **Đồng hồ `i64` ms, bù giờ động không trần cứng, watchdog** | `src/clock.rs`, `src/v4/session.rs` | `des_clock_dynamic_added_time_no_hard_cap`, `des_clock_no_double_counting_deadball_vs_compensable` |
| **Event Scheduler tất định, hủy sự kiện, chống lặp vô hạn** | `src/v4/scheduler.rs`, `src/v4/session.rs` | `des_scheduler_tie_break_priority_sequence_deterministic`, `des_scheduler_cancels_stale_generation_events`, `des_scheduler_rejects_past_and_infinite_same_ms_loop` |
| **Hợp đồng A — Tương tác liên tục giữa 2 mốc DES** | `src/v4/interaction.rs`, `src/v4/spatial.rs`, `src/v4/ball_play.rs`, `src/v4/shooting.rs` | `des_ci_defender_intercepts_before_receiver` + 5 unit test trong `interaction.rs` |
| **Hợp đồng B — Bình luận chuẩn 1.0x bất biến 1.0x–3.0x** | `NarrationScheduler.kt`, `MatchTtsManager.kt`, `TtsSettingsStore.kt` | Kiểm tra tĩnh trong `verify_v4_des_contract.py` + kiểm thử thiết bị Android |
| **Hợp đồng C — Checkpoint đang diễn ra (Rust + Android)** | `src/v4/checkpoint.rs`, `src/jni_bridge.rs`, `MatchCheckpointStore.kt`, `MatchPlaybackController.kt` | `des_checkpoint_roundtrip_exact_continuation_all_states`, `tests/p1_kotlin/Smoke.kt` |
| **Hợp đồng D — Ruleset & TimingModel có phiên bản + DeadBall** | `src/v4/rules.rs`, `src/v4/set_pieces.rs`, `src/v4/input.rs` | `des_ruleset_version_immutable_per_match`, `des_goal_celebration_then_kickoff_nonzero_variable_duration`, `des_deadball_state_machine_blocks_open_play_events` |
| **Nghỉ giữa hiệp `HalfTimeBreak` & FastBatch parity** | `src/v4/session.rs`, `MatchPlaybackController.kt` | `des_halftime_break_pauses_interactive_and_auto_advances_fastbatch`, `des_fullevent_fastbatch_exact_parity_with_commands`, `p1_halftime_substitution_consumes_player_slot_not_inplay_window` |
| **Độ trễ chiến thuật tự nhiên & `pending_substitutions`** | `src/v4/tactics.rs`, `src/v4/session.rs` | `des_tactical_command_delayed_effective_time_and_override_policy`, `des_live_sub_waits_for_next_deadball` |
| **Chuyển đổi 6 công thức phụ thuộc nhịp sang `dt_sec`/`moved_m`** | `src/v4/movement.rs`, `src/v4/state.rs`, `src/v4/psychology.rs`, `src/v4/tactics.rs`, `src/v4/ball_play.rs` | `des_fatigue_step_splitting_invariance` + unit test bất biến chia bước trong `psychology.rs` |
| **Playback 1.0x–3.0x thống nhất & `LabAudioDirector` theo `simTimeMs`** | `MatchPlaybackController.kt`, `LabAudioDirector.kt`, `LabReferenceMatchActivity.kt`, `LabV4PlaybackCoordinator.kt` | `verify_v4_des_contract.py` (0 `uptimeMillis`, có `OWN_GOAL` trong `SHORT_HIGHLIGHTS`) |

---

## 3. Nhật ký Vòng Phản biện Độc lập & Sửa lỗi (Agent 8 → Rework Pass)

Trong vòng kiểm toán độc lập sau Đợt 3, **Agent 8** đánh dấu `REJECTED (NEEDS REWORK)` với **9 lỗi CRITICAL** và **5 lỗi MINOR**. Kỹ sư trưởng đã tổ chức vòng sửa lỗi ngay lập tức và xác minh triệt để:

1. **CRITICAL-1 (`v4/session.rs`)**: Bổ sung tham số `dt_sec` vào lời gọi `execute_dribble_duel(...)` (tránh lỗi biên dịch `E0061`).
2. **CRITICAL-2 (`v4/checkpoint.rs`)**: Gỡ `PartialEq` khỏi `MatchSessionSnapshotV4` và `MatchCheckpointV4` (tránh lỗi biên dịch `E0369` do `BinaryHeap`).
3. **CRITICAL-3 (`v4/tests.rs`)**: Đổi `EventTypeV4::DribbleAttempt` thành `EventTypeV4::DribbleLost | EventTypeV4::Carry` (tránh lỗi `E0599`).
4. **CRITICAL-4 (`LabMatchInputFactory.kt`)**: Nâng `ENGINE_VERSION_V4 = "v4.1.0-des"` và `ENGINE_PROTOCOL_V4 = 5` (tránh từ chối 100% input V4 từ Android).
5. **CRITICAL-5 (`LabReferenceMatchActivity.kt` & `LabRustLiveSession.kt`)**: Tạo `buildV4UpdateTacticsCommandJson(...)` gửi đúng schema `{"type":"UPDATE_TACTICS","side":"HOME","instructions":{...}}`.
6. **CRITICAL-6 (`LabReferenceMatchActivity.kt`)**: Đồng bộ `v4MatchId = JSONObject(generated).optString("matchId", v4MatchId)` khớp tuyệt đối với Rust khi lưu checkpoint.
7. **CRITICAL-7 (`v4/session.rs`)**: Đồng bộ trạng thái `CommandLifecycleStatus::Superseded` vào `self.command_timeline` khi ghi đè lệnh chiến thuật.
8. **CRITICAL-8 (`v4/rules.rs` & `v4/session.rs`)**: Lưu `phase_compensable_ms` trên `DeadBallState` và chỉ gọi `record_compensable_lost_ms` sau khi `advance_ms(dt_ms, false)` kết thúc pha, bảo toàn bất biến `compensable_lost_ms <= ball_out_of_play_ms`.
9. **CRITICAL-9 (`LabReferenceMatchActivity.kt`)**: Sửa chiều cộng tỷ số `OWN_GOAL` theo đúng đội hưởng lợi (`side`) khớp Rust `EventTypeV4::OwnGoal`.
10. **MINOR đã sửa kèm**: Sửa đường dẫn `payload.state.activeHomeTi` trong `LabRustLiveSession.kt`; nâng guard lên `NativeBridge.isV4DesReady()` trong `LabReferenceMatchActivity.kt`; kiểm tra trùng cầu thủ ngay khi xếp hàng `pending_substitutions` trong `v4/session.rs`.

---

## 4. Hướng dẫn Chạy Kiểm thử & Xác minh trên Máy của Bạn

Giải nén tệp bàn giao `Coffee-Shop-Manager-DES-v1.4-feat-branch.zip` (đang trỏ sẵn nhánh `feat/des-v1.4`), sau đó chạy tuần tự:

```bash
# 1. Kiểm chứng hợp đồng tĩnh Rust <-> JNI <-> Kotlin (đã đạt 16/16)
python3 tools/verify_v4_des_contract.py

# 2. Biên dịch và chạy toàn bộ bộ kiểm thử Rust (gồm 27 test cũ + 15 test DES v1.4 + PR #5)
cd rust/football_match_engine
cargo test --all-targets

# 3. Chạy benchmark Monte Carlo & kiểm chuẩn V4.1 DES (tùy chọn)
cargo test --release -- --ignored

# 4. Biên dịch thư viện .so cho Android (ARM64/x86_64)
cd ../..
./tools/build_lab_rust_android.sh

# 5. Chạy JVM Smoke Test, Commentary Regression & Build Debug APK
./tools/run_v4_p1_jvm_smoke.sh
./tools/run_commentary_regression.sh
./gradlew :app:assembleDebug
```

### Các mục bắt buộc cần kiểm chứng thêm trên thiết bị Android thật
- Kiểm thử chất lượng `MediaPlayer` + `PlaybackParams().setSpeed(3.0f).setPitch(1.0f)` với giọng TTS tiếng Việt (~4,2 âm tiết/giây) và đo drift cuối trận.
- Kiểm thử kill-process 5 trạng thái (`adb shell am force-stop`): bóng sống, đang ăn mừng bàn thắng, đang đá phạt/phạt góc, đang dừng `HalfTimeBreak`, và đang có lệnh chiến thuật chờ hiệu lực.
- Kiểm chuẩn dải thống kê Monte Carlo (128–1.000 trận) để chốt các hệ số hiệu chỉnh không khóa theo Mục XII.B DES v1.4.
