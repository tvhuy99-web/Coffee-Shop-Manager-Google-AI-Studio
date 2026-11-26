# Kiến trúc PingFootball 4.0 Foundation

## 1. Phạm vi và hiện trạng

Mô phỏng trận đấu theo DES, gồm đội hình, 54 thuộc tính, điều kiện trận, hai hiệp 45 phút cộng bù giờ, hiệp phụ và luân lưu theo thể thức; lệnh tương tác, thay người, trạng thái nghỉ giữa hiệp, kết quả, thống kê và replay/checkpoint. Career dài hạn, tài chính, chuyển nhượng, lịch giải nhiều mùa nằm ngoài lõi trận. Các module dưới đây hiện có mã hoạt động, nhưng ranh giới nghiệp vụ chưa phải tất cả đã đạt thiết kế đích.

## 2. Workspace Rust thực tế

[Cargo.toml](../rust/Cargo.toml) khai báo 10 crate. Đường dẫn `rust/football_match_engine` có package name `match_engine`, không phải một engine thứ hai.

| Crate | Đường dẫn từ rust/ | Trách nhiệm hiện hành |
|---|---|---|
| match_contracts | crates/match_contracts | Kiểu chung: 54 thuộc tính, vector, đơn vị, event/rule types, team instructions, observation và trait ports |
| match_physics | crates/match_physics | Quỹ đạo bóng 3D, contact prediction, pass reach, ground contact, loose-ball recovery |
| match_rules | crates/match_rules | Kiểu và hàm chính sách luật, lifecycle bóng chết, thời gian và thể thức |
| player_condition | crates/player_condition | Hai bể fatigue, tích phân tải, phục hồi, form và morale |
| match_tactics | crates/match_tactics | Độ trễ/lan truyền chỉ đạo, nội suy, pressing theo pha |
| player_intelligence | crates/player_intelligence | Policy quyết định, carry/hold, hỗ trợ, phòng ngự, thủ môn, tackle, timing và utility |
| coach_intelligence | crates/coach_intelligence | Đề xuất team instructions, lựa chọn cầu thủ ra/vào |
| match_telemetry | crates/match_telemetry | Attacking-chain, timing, diagnostics và đo lường |
| match_engine | football_match_engine | World State, scheduler, RNG, điều phối policy/physics/rules, commit, command, checkpoint, result và replay |
| football_match_engine_jni | crates/football_match_engine_jni | JSON/JNI, handle registry, cầu nối native, một cdylib |

DAG nội bộ được CI kiểm tra từ cargo metadata:
- match_contracts không phụ thuộc crate nghiệp vụ nội bộ.
- Bảy crate lá (physics, rules, condition, tactics, PI, coach, telemetry) chỉ phụ thuộc match_contracts trong nội bộ.
- match_engine phụ thuộc contracts và bảy crate lá.
- JNI chỉ phụ thuộc engine và contracts trong nội bộ, xuất libfootball_match_engine.so. Không đổi tên thư viện/ABI tùy tiện.
- Harness kiểm thử có thể dùng nhiều crate; không kéo phụ thuộc test vào production DAG.

## 3. Engine và dòng dữ liệu

```text
MatchInputV4 + seed + lịch lệnh
    -> MatchSessionV4 (World State + clock + RNG + scheduler)
    -> Observation / policy / dự báo -> thực thi vật lý -> quyết định luật
    -> kiểm tra bất biến -> commit trạng thái/sự kiện
    -> MatchResultV4 / checkpoint / diagnostics
    -> JNI -> Android playback -> commentary/audio
```

Đây là sơ đồ trách nhiệm, không phải lời khẳng định mọi bước đã có API delta độc lập. Các resolver hiện còn nằm trong engine.

- `rust/football_match_engine/src/v4/session.rs`: MatchSessionV4; new, submit_command, start_next_period, advance_steps, advance_until_ms, run_to_full_time, export_checkpoint, from_checkpoint.
- `v4/session_parts/`: 01_world, 02_stoppage, 03_actions, 04_period, 05_condition, 06_telemetry. Các phần vẫn thuộc session; không phải sáu crate độc lập.
- `v4/state.rs`, `scheduler.rs`, `transition.rs`: trạng thái authoritative, hàng đợi và kiểm tra commit. Public mutation dùng candidate riêng trước khi commit; có kiểm tra thời gian/sequence, dữ liệu hữu hạn, sở hữu bóng, roster, movement budget, ledger, goal delta. Không tuyên bố đã kiểm tra mọi invariant tại mọi nhánh.
- `clock.rs`: đồng hồ DES i64 ms. 2.700.000 ms/hiệp chính, 900.000 ms/hiệp phụ; thời gian nghỉ giữa hiệp là chuyển trạng thái, không cộng 15 phút nghỉ vào giờ thi đấu.
- advance_until_ms xử lý sự kiện đến horizon, có thể trả sim_time_ms nhỏ hơn target; polling không tự tạo khoảng vận động hay tiêu thụ RNG.
- FullEvent giữ event stream; FastBatch fingerprint không cần lưu toàn bộ event vector. Cùng đầu vào/lịch lệnh phải giữ kết quả và continuation.
- MatchCommandV4: thay đổi chiến thuật có thời gian lan truyền; thay người theo cửa sổ, hỗ trợ batch, nghỉ giữa hiệp và start-next-period.
- VersionManifest4 hiện ở `src/contracts/version.rs`: tách BuildIdentity, DataSchemas, GameplayIdentity, MatchConfigIdentity, PresentationIdentity. Không đồng nhất Cargo version với behavior ID; không đổi ID chỉ vì dọn tài liệu.

## 4. Ranh giới cần tiếp tục hoàn thiện

| Khu vực | Hiện trạng / giới hạn |
|---|---|
| Contracts | Có leaf crate nhưng một số DTO và manifest vẫn ở engine; tránh di chuyển máy móc hoặc re-export world state ra leaf. |
| Physics | Có các cơ chế quỹ đạo, reach, contact, recovery; tích hợp movement, ball_play, shooting, goalkeeper và set_pieces vẫn ở engine. Chưa kết luận chuyển động thống nhất hay đầy đủ cơ học bóng. |
| Rules/Condition | Có policy và fatigue thật; điều phối pháp lý, psychology, injury và vòng đời còn tích hợp engine. |
| PI | DecisionObservation và trait ports hiện hữu. EngineForecastAdapter đọc MatchStateV4/input để giữ dự báo tương thích; các trường như first_touch, heading, opponent_line_height cần kiểm toán quyền quan sát. Chưa đạt no-omniscience đầy đủ. |
| Coach | CoachObservation và chọn chiến thuật/thay người có ở crate riêng; coach_adapter gọi ai_manager với state mutable. Chưa tách hoàn toàn proposal/commit và đường command chung. |
| World State | Đã chia session và có transaction guard; chưa chứng minh tất cả trách nhiệm và invariant tách sạch. |
| Tactics | Có pha và policy lan truyền; phối hợp role/duty và chuyển trạng thái đầy đủ còn là mục tiêu phát triển. |
| Telemetry | Có diagnostics và đo lường; không được ảnh hưởng gameplay, RNG hoặc quyết định. |

## 5. Android: ba phân hệ logic

Các ranh giới là package/source group trong app, chưa phải ba Gradle module độc lập. Đường dẫn dưới `app/src/main/java/pl/mkrstudio/truefootball3/`:

- `lab/match_playback/`: MatchPlaybackController, LabRustLiveSession, LabV4PlaybackCoordinator, MatchCheckpointStore, LabGlobalSeedReplayStore, roster/stats projection. Quản lý clock trình diễn, viewing mode, checkpoint, restore và gửi lệnh Rust.
- `lab/match_commentary/`: LabCommentaryEngine, LabCommentaryTemplatePolicy, LabNarrativeTracker, MatchEventCueAdapter, NarrationScheduler; `audio/match_commentary/MatchTtsManager.kt`: TTS/WAV. Chỉ mô tả facts đã có, không bịa cầu thủ, tỷ số hoặc hành động.
- `lab/match_audio/` và `audio/match_audio/`: LabAudioDirector, MatchAudioManager, SoundCatalog, SoundPackManager; cue, SFX, khán đài, sound pack.
- `lab/NativeBridge.kt`: JNI; `lab/LabReferenceMatchActivity.kt`: UI/dialog và orchestration. `presentation/`: session boundary/generation gate. Không giả định Activity đã hoàn toàn không chứa orchestration.

Tốc độ trình diễn 1.0x/1.25x/1.5x/2.0x/3.0x không đổi gameplay. Mục tiêu FIFO cue, không auto-mute, báo backlog minh bạch. Speculative rollback chưa được công nhận là hoàn chỉnh.

## 6. Dữ liệu và build

54 thuộc tính: 13 technical, 14 mental, 8 physical, 9 goalkeeping, 10 hidden; thang 1–20. Base attributes bất biến trong trận; match condition và effective capability khác base. Career hooks không tự nhận có runtime impact. Dữ liệu england_v2 mang tính thử nghiệm; không gán là roster hoặc lịch thi đấu thật.

Rust toolchain 1.100.0, Cargo.lock, Android compileSdk 35/minSdk 26/targetSdk 29. JDK 17, NDK 27.2.12479018 trong CI. Debug package pl.mkrstudio.truefootball3.debug, ARM64; native protocol 5, 16 JNI exports được CI kiểm tra. Những giá trị định danh cụ thể lấy từ manifest/mã và cấu hình đang chạy, không đổi để đồng bộ tên tài liệu.

Xem [TESTING](TESTING.md) cho bằng chứng thực tế và [ROADMAP](ROADMAP.md) cho kiến trúc đích.

Hợp đồng không gian và số học chi tiết: [WORLD_CONTRACTS](WORLD_CONTRACTS.md).
