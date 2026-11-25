# PingFootball 4.0 — Theo dõi tái cấu trúc

Ngày khởi tạo: 25/11/2026. Đặc tả: [Revision 3](FOOTBALL_SIMULATION_4_SPEC.md).
Nhánh tích hợp: `refactor/football-simulation-4`. Main tại thời điểm nhập: `c2c392d8b3ccc57985e87337e8994fd7033328ee` (PI 4.0.28).
Đây là bảng công việc, không thay thế đặc tả và không phải biên bản nghiệm thu. ADR-001–011 đang chờ chủ dự án phê duyệt; dùng mốc 4.0.28 cho nhánh chuẩn bị, không tự coi ADR đã chốt.

## A-1 — IN PROGRESS (25/11/2026)

### Đã thực hiện và kiểm chứng
- [x] Hai commit gốc tồn tại ở nhánh `baseline/pi-4.0.27`, `baseline/pi-4.0.28`. Main vẫn ở `c2c392d8`; chưa có tag/protection.
- [x] Ghim Rust 1.100.0 theo log job 138915898432; NDK 27.2.12479018, JDK 17, Gradle 8.9, compileSdk 35, targetSdk 29 được ghi nhận.
- [x] Lưu 19 ZIP lịch sử có SHA-256/CRC và commit gốc đúng trên nhánh `evidence/a1-historical`, commit `121c69fe1569e85c25e6ed2487fd6a0013e63186`. Hai bộ population 1.384 trận, audit 4.0.27 đủ 904.
- [x] [Run 45593874053](https://github.com/tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio/actions/runs/45593874053): 14/14 jobs PASS, 1.384 FastBatch + 12 FullEvent exact parity (hai lần chạy), 904 audit PI 4.0.28 (5 nhóm), 3 checkpoint tests, negative control kết quả.
- [x] [Run 45604255587](https://github.com/tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio/actions/runs/45604255587) PASS: 14 ZIP mới đã archive, commit `b82dbf0d4c01b100ec00a155e364fdae7bd1b7de`. Tổng 33 ZIP lịch sử và locked evidence đã lưu bền vững. Manifest tại nhánh archive.
- [x] Cargo.lock do Cargo sinh, SHA-256 `41d521b51131a20781790cb9ee4c62c967f34003866d50d97df2a3354eae9099`, commit `b6353ea66112821b24f295bb201739198b1eb089` tại rust/football_match_engine/Cargo.lock. Workspace rust/ chưa tồn tại; chuyển lockfile có kiểm chứng tại A5.
- [x] Inventory 34 legacy, 5 shared, 9 JNI, 10 workflow, 11 static checks (2 shared + 9 legacy), 50+4 thuộc tính, 48 OGG ở [baseline-inventory.json](evidence/a-1/baseline-inventory.json) và [attribute_contracts.json](evidence/a-1/attribute_contracts.json). Log [139134958569](https://github.com/tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio/actions/runs/45593934894/job/139134958569) xác minh 101/101 static, 48 checksum OGG, 895 mẫu bình luận mỗi locale và token parity. Static không chứng minh build/device.

### Chưa hoàn thành — không đánh dấu PASS
- [ ] [Run 45606252958](https://github.com/tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio/actions/runs/45606252958): rerun dùng chính Cargo.lock đã commit; thiếu file/hash sai phải fail, không generate-lockfile lại. Đã kích hoạt, còn queued khi kiểm tra.
- [ ] [PR #7](https://github.com/tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio/pull/7), [run 45604679705](https://github.com/tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio/actions/runs/45604679705): test-only deep golden normalized input, result, terminal state/RNG/scheduler/command ledger, 7 marker restore, strict field-path comparison. Còn queued; chưa có PASS. FastBatch hiện chỉ fingerprint event, không phải toàn bộ raw event stream; FullEvent 12 trận có raw stream. Cần đối chiếu độ bao phủ Mục 11.3 trước nghiệm thu.
- [ ] Archive và xác minh raw artifacts của các run mới sau khi hoàn tất; bổ sung event/state trace nếu chưa đủ 5 cấp.
- [ ] Tag/ref protection và chốt ADR-001/002/003/008. Không tự coi tag/ADR đã duyệt; không tự quyết các ADR ảnh hưởng UX/gameplay.

### Chuẩn bị A0 (chưa triển khai)
- [x] [Coverage map 10 workflow và triage](evidence/a0/CI_COVERAGE_AND_TRIAGE.md). Rust tổng 185 PASS, 2 FAIL (version literal và test giao bóng sau bàn), 8 ignored. P1 static 17/22, 5 lỗi đã phân loại theo mã nguồn thực tế. Hai hợp đồng thay người hàng loạt và chống ghi đè hiện có implementation; cần test hành vi trước khi kết luận thiếu tính năng.
- [ ] Chỉ tích hợp A0 khi A-1 đạt. Không nới gate hoặc thay đổi gameplay để làm xanh CI.

**Chặn A1–A6:** A-1 chưa đủ 5 cấp và CI rerun còn chờ runner. Không xóa mã, không áp patch toàn khối, không sửa gameplay.

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
