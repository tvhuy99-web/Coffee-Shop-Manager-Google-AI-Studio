# Kiểm thử, build và nghiệm thu

Nguồn thực thi: [pingfootball-acceptance.yml](../.github/workflows/pingfootball-acceptance.yml). Workflow chạy trên push main, các nhánh được cấu hình, PR tương ứng và workflow_dispatch. Không xem bảng dưới đây là kết quả CI hiện tại; mở run đúng head SHA.

## Cổng L0–L5

| Level | Kiểm tra thực tế | Điều kiện |
|---|---|---|
| L0 | Static contracts, data/audio, fixture integrity, toolchain/lock, cargo metadata DAG, legacy entrypoints, fake-adb harness, clippy | Không bỏ qua lỗi; rustfmt debt hiện được báo riêng, không gọi strict formatting PASS |
| L1 | cargo check/test workspace locked all-targets; isolated attribute/psychology, host transaction benchmark | Compile và tests thật, không có fail |
| L2 | 8 job golden: baseline 1000, balanced 128, high_press/low_block/strong_home/strong_away mỗi 64, FullEvent 12, checkpoint markers | 1.384 FastBatch + 12 FullEvent = 1.396 trận; 1.384 cross-mode, checkpoint exact, 0 sai khác không được duyệt |
| L3 | 5 nhóm audit 54 attributes; 1 job diagnostics (50 probes, Monte Carlo 128/1000 và attacking sample) | 904 simulations = 5×8 + 54×16; kiểm tra cặp/field/role, báo dữ liệu thật; không coi audit kỹ thuật là xác nhận realism |
| L4 | Kotlin/JVM, commentary, missing-tool negative controls; Android ARM64 Debug APK, signature/package, JNI ABI | Build/kiểm thử thật, đủ 16 JNI exports, APK hash |
| L5 | Điện thoại thật, đúng APK/commit, matrix thiết bị | Đo đầy đủ theo ngưỡng được chốt; không thay bằng fake-adb hoặc đánh giá cảm tính |

L0–L4 hiện có 19 jobs gồm aggregate bắt buộc. Không sửa golden, assertion hoặc tắt gate để làm xanh. Khi có behavior change chủ đích, giữ baseline và candidate độc lập; kiểm tra first divergence, paired delta và phê duyệt golden mới riêng, không yêu cầu behavior mới giống hệt baseline cũ sau khi đã phê duyệt. Cập nhật workflow version assertion cùng thay đổi behavior có chủ đích, không làm trong D/R.

## Chạy local

Tại repository root:

```sh
(cd rust && cargo metadata --locked --format-version 1)
(cd rust && cargo check --workspace --locked --all-targets)
(cd rust && cargo test --workspace --locked --all-targets)
(cd rust && cargo clippy --workspace --locked --all-targets)
(cd rust && cargo fmt --all -- --check)
python3 tools/verify_workspace_lock.py docs/evidence/a-1/frozen-Cargo.lock rust/Cargo.lock
python3 tools/verify_a1_boundary.py
python3 tools/verify_parity_fixtures.py
python3 tools/verify_phase_a_boundaries.py
bash tools/run_v4_p1_jvm_smoke.sh
bash tools/run_des_playback_jvm.sh
bash tools/run_commentary_regression.sh
bash tools/test_a8_device_harness.sh
bash tools/build_lab_rust_android.sh
bash gradlew :app:validateCommentaryTokens :app:assembleDebug --no-daemon
```

Cần Rust 1.100.0, Cargo.lock, Python 3, JDK 17, Kotlin compiler (CI 1.9.25), Android SDK 35/build-tools 35.0.1, NDK 27.2.12479018, cargo-ndk 4.1.2 cho ARM64. Dùng checkout đầy đủ lịch sử khi chạy verify_a1_boundary: nó xác minh dữ liệu bằng Git object sẵn có. Các fixture V3 dưới tests/fixtures/legacy chỉ kiểm tra integrity, không chạy Java engine. Không xóa vì tên lịch sử.

Golden và toolchain đối chứng được workflow đọc từ commit bằng chứng cố định. Không đổi đường dẫn, hash, seed, fixture hoặc bản ghi đối chứng nếu chưa đánh giá tác động. L2 kiểm tra normalized input, result, state/RNG/scheduler/command ledger, fingerprints và continuation. So sánh cross-mode chỉ chuẩn hóa các trường biểu diễn mode/cursor đã được hợp đồng cho phép.

## Kiểm chuẩn gameplay cho B1–B6

- Test cơ chế cô lập: unit, geometry, mirror, dt split, boundary/contact, offside, advantage, penalty, restart, fatigue, injury, rollback, command window.
- Ít nhất 1.000 trận ghép seed, cohort cân bằng/mirror, gradient sức mạnh, chiến thuật, đội hình vai trò, thiếu người, luật đặc biệt, môi trường và rare cases. Audit đủ 54 thuộc tính; phân biệt 50 runtime với 4 career hooks theo wiring test.
- Báo mean, median, SD, P5/P95, min/max, phân bố tỷ số, paired delta và độ bất định. Không bỏ outlier không giải thích. Chỉ so chuẩn realism với nguồn/giải/mùa rõ và định nghĩa metric khớp; không đặt quota từ trung bình.
- Lưu commit, toolchain, input/seed, logs, JSON/JSONL, SHA-256, first differing field, tác động version, rollback. Cổng xanh không thay bằng báo cáo tự viết.

## Thiết bị và hiệu năng

Host benchmark: FastBatch matches/s, FullEvent throughput, latency P50/P95/P99, RAM, checkpoint size/serialize/restore, JNI latency. Đo trên cấu hình khóa và ghi warmup/build.

L5: model/OS/ABI/RAM, APK hash, voice, mode/rate; 1.0x–3.0x, render/thread stall, ANR/crash, RAM/CPU/pin/nhiệt, AudioFocus, TTS backlog/drift/underflow, force-stop/restore, halftime, commands, replay và handle lifecycle. Dùng tools/capture_a8_device_evidence.sh theo thiết bị thực, không coi fake-adb là L5. Chốt ngưỡng trước đo. Trạng thái nghiệm thu thiết bị trước đây chỉ là chủ dự án chấp nhận sử dụng nền tảng, không chứng nhận L5 đầy đủ hoặc bản release.

## Quyết định nghiệm thu

PASS khi đúng commit, mọi job bắt buộc thành công và bằng chứng phù hợp. QUEUED/RUNNING, CANCELLED, SKIPPED hoặc thiếu log không phải PASS. Nếu CI thất bại, giữ nguyên gate, xác định lỗi nguồn hay môi trường rồi sửa nhỏ và chạy lại. Không công bố production release chỉ vì có Debug APK.

### B1.1 gate

`(cd rust && cargo test --workspace --locked --all-targets b11_ -- --nocapture)` chạy riêng các test không gian, số học, clock, scheduler và invariant mới. Cổng đầy đủ vẫn là workflow L0–L4, không thay bằng lệnh lọc. Xem [WORLD_CONTRACTS](WORLD_CONTRACTS.md#b116--nghiệm-thu) cho scope, giới hạn và rollback; chỉ nghiệm thu trên run đúng candidate.


### B1.2 candidate shadow report

`(cd rust/football_match_engine && B12_SHADOW_SEEDS=1000 B12_SHADOW_REPORT_PATH=/tmp/b12-shadow.json cargo test --release --locked --lib b12_shadow_locomotion_report -- --ignored --nocapture)`

`python3 tools/verify_b12_shadow_report.py /tmp/b12-shadow.json /tmp/b12-shadow-summary.json`

L1 chạy hai lệnh tương đương và lưu artifact raw/summary. 1.000 full matches là legacy reference, 1.000 pair chỉ là interval; không phải paired full-match acceptance của gameplay B1.2. Xem [PLAYER_LOCOMOTION](PLAYER_LOCOMOTION.md) cho production gate còn mở. Không sửa golden để bỏ qua gate.


### B1.2 hybrid full-match experiment

Workflow riêng `B1.2 experimental locomotion evidence` chạy 1.000 cặp full matches, shard 0..3 mỗi shard 250. Test-only exp1 có identity riêng, release không chọn được và không restore checkpoint exp1. `tools/verify_b12_experiment_report.py evidence` tổng hợp đủ shard, lưu lỗi và paired distributions; không thay L0–L4 hoặc gate production. Cặp thất bại vẫn xuất raw artifact. Cross-mode exp1 chạy riêng trên shard 0; checkpoint resume trong L1. Phạm vi hybrid và giới hạn xem PLAYER_LOCOMOTION.
