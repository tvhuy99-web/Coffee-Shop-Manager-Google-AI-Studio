# PingFootball 4.0 — Theo dõi tái cấu trúc

Ngày khởi tạo: 25/11/2026. Đặc tả: [Revision 3](FOOTBALL_SIMULATION_4_SPEC.md).
Nhánh tích hợp: `refactor/football-simulation-4`. Main tại thời điểm nhập: `c2c392d8b3ccc57985e87337e8994fd7033328ee` (PI 4.0.28).
Đây là bảng công việc, không thay thế đặc tả và không phải biên bản nghiệm thu. ADR-001–011 đang chờ chủ dự án phê duyệt; dùng mốc 4.0.28 cho nhánh chuẩn bị, không tự coi ADR đã chốt.

## A-1 — IN PROGRESS (25/11/2026)

### Đã thực hiện và kiểm chứng
- [x] Hai commit gốc tồn tại ở nhánh `baseline/pi-4.0.27`, `baseline/pi-4.0.28`. Main vẫn ở `c2c392d8`.
- [x] Rust 1.100.0 ghim theo log job 138915898432. NDK 27.2.12479018, JDK 17, Gradle 8.9, compileSdk 35, targetSdk 29 ghi nhận.
- [x] Cargo.lock SHA-256 `41d521b51131a20781790cb9ee4c62c967f34003866d50d97df2a3354eae9099`, commit `b6353ea66112821b24f295bb201739198b1eb089` tại rust/football_match_engine/Cargo.lock (crate hiện hành, chưa có workspace; chuyển workspace tại A5).
- [x] Inventory 34 legacy, 5 shared, 9 JNI, 10 workflow, 11 static checks (2 shared + 9 legacy), 50+4 attributes, 48 OGG ở [baseline-inventory.json](evidence/a-1/baseline-inventory.json) và [attribute_contracts.json](evidence/a-1/attribute_contracts.json).
- [x] [Run 45593874053](https://github.com/tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio/actions/runs/45593874053): 14/14 PASS trên candidate lock, 1.396 exact parity, audit 904 trận PI 4.0.28, 3 checkpoint tests và negative result control.
- [x] [Run 45606252958](https://github.com/tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio/actions/runs/45606252958): 14/14 PASS trên Cargo.lock đã commit, 101/101 static, 48 OGG checksum, 895 templates/locale, 1.384 FastBatch + 12 FullEvent, 5 nhóm audit 904, checkpoint tests. Không tính hai lượt lặp là số seed mới.
- [x] [Run 45604679705](https://github.com/tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio/actions/runs/45604679705): 8/8 PASS, normalized input, result, initial/terminal state, RNG, scheduler, commands, 7 checkpoint markers thật (live, pass_flight, tackle_loose, goal_celebration, pending_tactics, halftime, period_end). Hai lượt raw JSONL byte-identical, 4 negative controls mỗi cohort. FastBatch event fingerprint; 12 FullEvent raw events. First differing field comparator; chưa coi đầy đủ raw event cho mọi seed.
- [x] [Run 45611090146](https://github.com/tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio/actions/runs/45611090146) PASS: xác minh ZIP hash, member hash, baseline, lockfile, harness commit và raw parity; 22 ZIP mới lưu tại commit `7bdc6afe4f64304bf929095c4b54655d6d84b5eb` nhánh evidence/a1-historical. Tổng 55 ZIP (19 lịch sử + 14 candidate + 22 committed/deep). 
- [x] Thêm test-only replay toàn bộ 1.384 normalized FastBatch inputs dưới FullEvent; so event fingerprint, result, state/RNG/scheduler/commands với deep golden; capture toàn bộ events, hai lần byte-identical, gzip shard lossless 50 trận. [Run 45611578006](https://github.com/tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio/actions/runs/45611578006) queued khi kiểm tra, chưa PASS. [PR #7](https://github.com/tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio/pull/7).
- [x] Workflow tạo 2 tag baseline xác định, không force, không publish release: [run 45611607605](https://github.com/tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio/actions/runs/45611607605) queued; chưa xác minh tag tồn tại.

### Cổng A-1 còn thiếu
- [ ] Full event trace 1.384 FastBatch shadow replays PASS và raw trace archive bền vững. Xác minh checkpoint state/command trace coverage Mục 11.3; module first-divergence attribution chưa tự động, report ghi unclassified nếu chưa xác định.
- [ ] Xác minh tag thực tế và bảo vệ ref/tag; xác nhận ADR-001/002/003/008. Không tự coi tag/ADR đã được chủ dự án duyệt.
- [ ] Tích hợp PR #7 sau các gate. Không thay đổi golden để làm xanh lỗi.

## A0 — chuẩn bị song song trên nhánh riêng, chưa merge

- [x] [Coverage map 10 workflow và triage](evidence/a0/CI_COVERAGE_AND_TRIAGE.md).
- [x] [PR #8](https://github.com/tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio/pull/8) test/CI only: manifest độc lập version + checkpoint negative cases, giữ nguyên 64 seed API/RNG; fail cứng khi thiếu công cụ; sửa 5 static assertion lỗi thời/khác implementation, bắt buộc JVM DES, thêm pause và archive-byte regressions.
- [x] Thêm diagnostic goal/restart boundary 71001..71010. Test P0 gốc vẫn nguyên, chưa kết luận nguyên nhân. Rust tổng 185 PASS, 2 FAIL, 8 ignored; P1 static cũ 17/22.
- [ ] [Run 45611400488](https://github.com/tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio/actions/runs/45611400488) queued; chờ actual test results, phân loại goal restart, không giả PASS.
- [ ] Hoàn tất A-1 trước khi merge A0. Sau đó hợp nhất L0–L4 và chạy đầy đủ Rust/Android/APK evidence, sửa production riêng chỉ nếu bằng chứng cho thấy thiếu nghiệp vụ.

**Chặn A1–A6:** chưa đóng A-1. Không xóa legacy, không áp patch toàn khối, không đổi gameplay hay merge main.

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
