# PingFootball 4.0 — Theo dõi tái cấu trúc

Ngày khởi tạo: 25/11/2026. Đặc tả: [Revision 3](FOOTBALL_SIMULATION_4_SPEC.md).
Nhánh tích hợp: `refactor/football-simulation-4`. Main tại thời điểm nhập: `c2c392d8b3ccc57985e87337e8994fd7033328ee` (PI 4.0.28).
Đây là bảng công việc, không thay thế đặc tả và không phải biên bản nghiệm thu. ADR-001–011 đang chờ chủ dự án phê duyệt; dùng mốc 4.0.28 cho nhánh chuẩn bị, không tự coi ADR đã chốt.

## A-1 — IN PROGRESS (25/11/2026)

### Đã thực hiện và kiểm chứng
- [x] Hai commit gốc tồn tại và được giữ ở nhánh `baseline/pi-4.0.27`, `baseline/pi-4.0.28`. Main vẫn ở `c2c392d8`. Nhánh không phải tag bất biến; chưa có tag/protection.
- [x] Ghim Rust 1.100.0 theo log job [138915898432](https://github.com/tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio/actions/runs/45527962031/job/138915898432), commit `d5f4336`. NDK 27.2.12479018, JDK 17, Gradle 8.9, compileSdk 35, targetSdk 29 được ghi trong hồ sơ; chưa kiểm thử build Android.
- [x] Tải 19 ZIP lịch sử từ ba run 45520666910, 45520666555, 45527962031. SHA-256 khớp 19/19 GitHub digest; kiểm tra CRC ZIP và commit.txt 19/19. Hai bộ DES mỗi bộ 1.384/1.384 trận, 0 failed/invalid, seed duy nhất; audit 4.0.27 đủ 54 thuộc tính/904 trận. Đây là bằng chứng lịch sử, không phải golden khóa mới.
- [x] [Manifest lịch sử](evidence/a-1/historical-artifacts.json) có ID, digest, expiry. Bản sao ZIP đã tải và kiểm chứng ở phiên làm việc; SHA-256 gói tổng: `8c46434d33d8f52d6d7f0b64ef84740a92291bf4cdc6f5594c645da534754b64`.
- [x] Thêm workflow [tái lập A-1](../.github/workflows/a1-baseline-freeze.yml) trên chính commit 4.0.28; sinh lockfile candidate, reuse lockfile đã commit, hai lần chạy 1.384 FastBatch + 12 FullEvent, negative result control, audit 5 nhóm/904 trận, checkpoint continuation; xuất SHA-256. Đã commit workflow, chưa có log chạy mới được xác nhận.
- [x] Tạo nhánh `evidence/a1-historical` và workflow [archive](../.github/workflows/a1-archive-historical.yml): tải ZIP gốc, xác minh lại và push không force chỉ vào nhánh archive. Chưa xác nhận commit ZIP của workflow.

### Cổng A-1 còn thiếu (không đánh dấu PASS)
- [ ] Tag bất biến hoặc bảo vệ ref; xác nhận chính thức ADR-001, ADR-002, ADR-003, ADR-008. Dùng mặc định 4.0.28 cho công việc có thể rollback; không tự quyết các ADR về UX/gameplay.
- [ ] Xác nhận Actions chạy thành công; lấy Cargo.lock do Cargo sinh, kiểm tra rồi commit tại `rust/football_match_engine/Cargo.lock` (crate hiện hành, chưa có workspace). Khi A5 chuyển workspace, giữ lockfile baseline và chuyển lockfile workspace có kiểm chứng. Chạy lại --locked với lockfile đã commit. Không giả tạo lockfile bằng tay.
- [ ] Tái lập golden với seed/input/command log và 5 cấp Mục 11.3 (bao gồm state, RNG, scheduler/ledger và checkpoint đầy đủ); comparator hiện tại mới bao phủ JSON đã export và negative control mức kết quả. Cần bổ sung test-only instrumentation và first-divergence theo field path.
- [ ] Chứng minh audit 904 trận trên đúng 4.0.28, checkpoint/replay ở các trạng thái yêu cầu, inventory 34 tệp legacy/9 JNI/10 workflow/11 static asserts/50+4 attribute/48 OGG/895 templates.
- [ ] Kiểm tra archive commit thực tế trên GitHub và lưu golden mới bền vững.

**Chặn A0/A1–A6:** runner nội bộ hiện không có Rust/Cargo và không clone được GitHub. Không áp patch, không xóa mã, không sửa gameplay trước khi cổng A-1 đạt. Các workflow được đẩy lên nhánh tích hợp, không coi việc commit workflow là kết quả CI.

## Trạng thái thực tế
- [x] Nhập nguyên văn đặc tả Revision 3.
- [x] Bảo tồn [patch A1–A6 gốc](patches/pingfootball_a1_a6.patch), **chưa áp dụng**.
- [ ] A-1 — Freeze hai baseline 4.0.27/4.0.28; ghim Rust và Cargo.lock; lưu raw JSON, 904 trận audit, golden per seed/checkpoint, manifest hash.
- [ ] A0 — Sửa version test theo hợp đồng manifest + negative case; 5 P1 checks; fail cứng khi thiếu công cụ; coverage map L0–L4, chạy Rust, 1.396 trận, audit 54 thuộc tính, APK ARM64.
- [ ] A1 — Archive bằng chứng, dọn legacy theo nhóm; exact parity.
- [ ] A2 — Contracts, đơn vị, version manifest và tương thích checkpoint.
- [ ] A3 — Cổng dữ liệu PI/Coach, trait port, policy parity.
- [ ] A4 — Biên World State, commit nguyên tử, invariants, checkpoint.
- [ ] A5 — 10 crate production thật, integration harness, DAG, workspace locked, full golden.
- [ ] A6 — JNI giữ ABI/.so; Android playback/commentary/audio, Kotlin/JVM, cue trace, APK.
- [ ] A7 — 1.396 trận + 12 FullEvent, command scenarios, parity, checkpoint, audit 54, benchmark, L0–L4.
- [ ] A8 — Nghiệm thu thiết bị thật theo ADR-009/L5; nếu không có thiết bị, ghi BLOCKED.
- [ ] B1 — Physics + Rules + Condition.
- [ ] B2 — Player Intelligence, observation hữu hạn.
- [ ] B3 — Match Tactics, 4 pha, phối hợp tập thể.
- [ ] B4 — Coach Intelligence qua MatchCommand chung.
- [ ] B5 — Commentary/Audio và lookahead nếu ADR-006 duyệt.
- [ ] B6 — Nghiệm thu toàn hệ sinh thái.

## Kiểm tra tĩnh patch được cung cấp
- 185 tệp, 3.038 dòng thêm, 24.277 dòng xóa (git apply --stat).
- Git blob SHA-1 `90907b41547365918f6cb394e4c834e5d566c6af`; SHA-256 `590fd92fb83cc9b4473c93e94a140729076ac5d1b14c4591aa009a9cac67f63b`.
- Patch xóa tài liệu PI/DES, log repair-evidence, Java golden fixtures và workflow cũ; phải bảo tồn archive và coverage trước khi xóa.
- Workspace patch tạo match_engine và football_match_engine_jni; chưa đạt 10 crate theo A5.
- Không có diff tạo rust/Cargo.lock hoặc rust/rust-toolchain.toml. Test version trong patch đổi literal 4.0.27 thành 4.0.28; chưa đủ hợp đồng A0.
- Patch chứa bản Markdown riêng của đặc tả (tạo mới cùng đường dẫn). Khi thử patch trên nhánh có tài liệu, phải tránh xung đột đường dẫn này.
- Chưa chạy git apply --check trên checkout của baseline, cargo test, parity, APK hay thiết bị. Không suy diễn PASS.

## Quy trình làm việc tiếp theo
1. Xác nhận ADR theo Mục 14.3; chọn baseline và giữ cả hai commit.
2. Hoàn thành A-1 trước khi xóa. Lập evidence manifest và giữ raw artifact có hash, seed/input, toolchain.
3. Hoàn thành A0 bằng commit độc lập T/I có test thực; không nới gate để làm xanh.
4. Thử patch trên nhánh thử nghiệm từ baseline, chỉ lấy từng phần theo A1–A6; không áp toàn khối lên main. Ghi rõ phần giữ, sửa, bỏ và lý do; từng PR có gate, rollback và bằng chứng.
5. Main chỉ nhận thay đổi đã nghiệm thu đúng cổng; chưa tăng PI 4.0.29. Bắt đầu B sau A7 và áp dụng A8 theo đặc tả.
