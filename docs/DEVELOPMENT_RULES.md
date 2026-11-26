# Hiến chương phát triển

Các nguyên tắc sau áp dụng từ nền tảng hiện tại. Mục tiêu chưa đạt không được biến thành tuyên bố về mã hiện hữu.

| ID | Quy tắc bắt buộc | Bằng chứng |
|---|---|---|
| HC-01 | Mechanism-first. Không quota/nắn tỷ số, sút, chuyền, lỗi, thẻ, thời gian bóng sống, handicap ngầm. Chỉ hiệu chỉnh tham số có nghĩa cơ học/hành vi; giới hạn luật/watchdog hợp lệ không phải quota. | Test cơ chế, giả thuyết, cohort |
| HC-02 | Refactor giữ exact parity; thay đổi công thức, thứ tự RNG, timing, policy, xác suất là behavior change. | First divergence, fingerprint, checkpoint |
| HC-03 | PI/Coach chỉ dùng Observation và RNG được cấp; không đọc hidden state, ý định đối phương, RNG kết quả. HLV AI/người chơi qua chung MatchCommand. Ngoại lệ hiện tại phải được chỉ rõ và đóng ở B2/B4. | Dependency/access audit, hidden-world equivalence, command parity |
| HC-04 | Rust authoritative, DES i64 ms. Rate, polling, batching, restore không đổi gameplay; chia dt khi điều kiện cố định theo hợp đồng số học. | Cross-mode, replay, dt, scheduler |
| HC-05 | Tách behavior ID, build, schema, config và presentation; không đổi ngầm checkpoint/replay; version 4.x.y theo module thực sự đổi. | VersionManifest4, compatibility/negative tests |
| HC-06 | Đổi Physics/Rules/Condition/Tactics/PI/Coach phải có isolated test, ít nhất 1.000 trận ghép seed, cohort bổ sung và audit 54 thuộc tính. | JSON gốc, paired delta, test logs |
| HC-07 | Không placeholder, fallback âm thầm hay test giả. Module mới phải có chức năng và caller thật. | Call sites, failure/negative tests |
| HC-08 | Phân biệt đã viết, build, kiểm thử, kiểm chuẩn và device acceptance; không có log không tự nhận PASS. | CI/job SHA và giới hạn |
| HC-09 | Engine sở hữu World State; module trả fact/proposal/delta có kiểu, commit nguyên tử; không áp dụng nửa giao dịch. | Fault injection, cancellation, checkpoint |
| HC-10 | Bảo toàn vật lý/luật: không teleport, hai chủ bóng, thực thi hai lần, ghi bàn trái luật. | Invariants, scenario/property tests |
| HC-11 | Chuẩn hóa đơn vị, hệ tọa độ, số hữu hạn, miền, tie-break; epsilon/clamp không dùng che lỗi; dung sai phải giải thích và version. | Geometry, mirror, dt, cross-target |
| HC-12 | Telemetry/Presentation chỉ quan sát; không ảnh hưởng RNG/gameplay; UI không sinh event gameplay. | Rate/telemetry parity |
| HC-13 | Giữ seed/input/toolchain/hash/fixture/bằng chứng cần cho kiểm thử. Không cần tạo tag/nhánh archive mới cho dọn tài liệu. Không làm mất đối chứng đang được CI dùng. | Artifact manifest, checksum, Git SHA |
| HC-14 | Đo CPU/RAM/latency/checkpoint/pin/nhiệt/backlog; không lược gameplay theo thiết bị/rate. | Host benchmark, device matrix |
| HC-15 | Thay đổi nhỏ, rollback được, cổng bắt buộc xanh trước nghiệm thu. | Diff, PR/commit, test evidence |

## Hợp đồng giao tiếp

- Physics trả facts/trajectory/contact/delta, không tăng tỷ số hoặc xác định luật. Phân biệt dự báo tiếp xúc, tiếp xúc thực và kiểm soát bóng.
- Rules quyết định hợp lệ, restart, advantage, offside, card, cửa sổ thay người và kết thúc hiệp/trận; Engine commit. Việc chạm bóng không mặc nhiên xác định sở hữu.
- Condition cập nhật theo tải thực và thời gian đúng một lần; hai bể fatigue có miền hữu hạn; injury không tự dừng trận; tâm lý không tạo bonus vô hạn.
- Tactics phân công tập thể và thời điểm hiệu lực; PI chọn ý định từ nhận thức; Coach lập kế hoạch và đề xuất lệnh.
- Contracts chỉ kiểu, schema, unit và port; không sở hữu World State, RNG master hoặc công thức nghiệp vụ.
- Commentary chỉ event đã commit, metadata có bằng chứng; thiếu dữ liệu chọn câu trung tính hoặc bỏ cue không đủ căn cứ. Không auto-mute khi tăng rate, không ép gameplay chờ lời bình. Audio/presentation không sửa kết quả.

## Version và thay đổi

Định danh behavior hiện tại được giữ trong code và manifest. Không tự reset version về 4.0.0. Thay đổi behavior cần ID mới đúng module và đánh giá checkpoint/schema; presentation ID độc lập. Dữ liệu không tương thích phải bị từ chối rõ, không silently fallback.

Mỗi thay đổi ghi ID roadmap, loại R/B/I/T/D, crate owner, baseline SHA, phạm vi/ngoài phạm vi, giả thuyết, test, version impact, kết quả và rollback. Với B, giữ đối chứng nguyên vẹn, báo sai khác có chủ đích và lý do; chỉ cập nhật golden sau review có bằng chứng. Với R/D, zero gameplay divergence.

Không tự áp đặt ADR chưa xác nhận. Các quyết định còn mở: ngưỡng và ma trận thiết bị, cross-platform bit-exact trước release, nguồn chuẩn realism, ruleset được hỗ trợ, speculative rollback/UX khi TTS backlog, chính sách tương thích replay cũ. Chốt khi hạng mục liên quan bắt đầu; không nới gate để né quyết định.
