# PingFootball 4.0 — Theo dõi tái cấu trúc

Ngày khởi tạo: 25/11/2026. Đặc tả: [Revision 3](FOOTBALL_SIMULATION_4_SPEC.md).
Nhánh tích hợp: `refactor/football-simulation-4`. Main tại thời điểm nhập: `c2c392d8b3ccc57985e87337e8994fd7033328ee` (PI 4.0.28).
Đây là bảng công việc, không thay thế đặc tả và không phải biên bản nghiệm thu. ADR-001–011 đang chờ chủ dự án phê duyệt; dùng mốc 4.0.28 cho nhánh chuẩn bị, không tự coi ADR đã chốt.

## A-1 — IN PROGRESS (25/11/2026)

### Đã thực hiện và kiểm chứng
- [x] Hai branch baseline và hai tag `pingfootball-baseline-pi-4.0.27` -> `1d7bf1b43a4384dff8b8ffbcb4953d18933e9393`, `pingfootball-baseline-pi-4.0.28` -> `c2c392d8b3ccc57985e87337e8994fd7033328ee` đã xác minh trực tiếp qua refs. [Run 45611607605](https://github.com/tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio/actions/runs/45611607605) PASS. Main chưa đổi; tag chưa được bảo vệ.
- [x] Rust 1.100.0 ghim theo log lịch sử; NDK 27.2.12479018, JDK 17, Gradle 8.9, compileSdk 35, targetSdk 29 ghi nhận. Cargo.lock SHA-256 `41d521b51131a20781790cb9ee4c62c967f34003866d50d97df2a3354eae9099`, commit `b6353ea66112821b24f295bb201739198b1eb089`; crate hiện hành chưa có workspace, chuyển lock tại A5.
- [x] Inventory 34 legacy, 5 shared, 9 JNI, 10 workflow, 11 static checks (2 shared + 9 legacy), 50+4 attributes ở [baseline-inventory.json](evidence/a-1/baseline-inventory.json) và [attribute_contracts.json](evidence/a-1/attribute_contracts.json).
- [x] [Run 45593874053](https://github.com/tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio/actions/runs/45593874053) 14/14 PASS candidate lock; [run 45606252958](https://github.com/tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio/actions/runs/45606252958) 14/14 PASS committed lock, 101/101 static, 48 OGG checksum, 895 templates/locale, 1.384 FastBatch + 12 FullEvent parity, audit 904 PI 4.0.28, checkpoint tests.
- [x] [Run 45604679705](https://github.com/tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio/actions/runs/45604679705) 8/8 PASS: normalized input, result, initial/terminal state/RNG/scheduler/commands, 7 checkpoint markers thật (live, pass_flight, tackle_loose, goal_celebration, pending_tactics, halftime, period_end). Raw JSONL byte-identical, 4 negative controls mỗi cohort.
- [x] [Run 45611090146](https://github.com/tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio/actions/runs/45611090146) PASS, 22 ZIP archive tại commit `7bdc6afe4f64304bf929095c4b54655d6d84b5eb` trên evidence/a1-historical. Tổng 55 ZIP (19 lịch sử +14 candidate +22 committed/deep).
- [x] [Run 45611578006](https://github.com/tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio/actions/runs/45611578006) 6/6 PASS: 1.384 FastBatch normalized input replay ở FullEvent, **4.320.279 gameplay events**, 0 mismatch, cross-mode result/state/RNG/scheduler/ledger parity, 2 independent raw traces byte-identical, lossless gzip shards.
- [x] [ADR proposals](evidence/a-1/ADR_PROPOSALS.md) ghi rõ khuyến nghị và thực tế triển khai.

### Cổng A-1 còn thiếu
- [ ] [Run 45615334394](https://github.com/tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio/actions/runs/45615334394): archive 6 full trace ZIP, xác minh member SHA-256, raw trace và chia ZIP >64MiB thành phần nguyên byte có hash ghép lại. Đang chờ CI; chưa tuyên bố 61 archive đã lưu.
- [ ] Xác nhận ADR-001/002/003/008 và bảo vệ tag/ref (rulesets API trả [] khi kiểm tra); first-divergence module attribution chưa tự động. Đánh giá độ bao phủ Mục 11.3 trước nghiệm thu.
- [ ] Tích hợp PR #7 sau gate; không thay golden để làm xanh lỗi.

## A0 — chuẩn bị trên nhánh riêng, chưa merge

- [x] [Coverage map 10 workflow và triage](evidence/a0/CI_COVERAGE_AND_TRIAGE.md).
- [x] [Run 45611400488](https://github.com/tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio/actions/runs/45611400488): version manifest + checkpoint negative cases PASS, P1 static 20/20 PASS. Kotlin JVM fail compile do JSONStub thiếu getInt và NativeBridge test double thiếu 6 API DES; không phải lỗi compile production được chứng minh.
- [x] Goal/restart diagnostic: 30 goal/own goal, 29 opponent kickoff đúng, 1 terminal boundary, 0 suspicious. Seed 71008 goal sequence 3105 at 5712450 ms, FULL_TIME sequence 3106 at 5715000 ms. ZIP artifact 15020102586 SHA-256 `8cb0db02c06018f81c2f3a6f971fabf64980174bfd3506c8abf7769efc50246f` đã tải và xác minh.
- [x] [PR #8](https://github.com/tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio/pull/8) test-only: sửa goal restart assertion dựa trên dữ liệu, thêm negative cases mất kickoff, sai side/spot, timeline bị cắt; cho phép boundary hợp lệ, không bỏ qua live play trước restart. Cập nhật test stubs; unexpected JNI mock calls fail thay vì fake PASS.
- [ ] [Run 45615354266](https://github.com/tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio/actions/runs/45615354266): rerun targeted goal invariant, version, P1 static, Kotlin smoke/DES và missing-tool controls; queued. Chờ kết quả actual.
- [ ] Hoàn tất A-1 trước merge A0. Hợp nhất L0–L4; chạy Rust all-targets, evidence và Android ARM64/APK. Không sửa production nếu không có bằng chứng thiếu nghiệp vụ.

**Chặn A1–A6:** chưa đóng A-1. Không xóa legacy, không áp patch toàn khối, không đổi gameplay, không merge main.

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
