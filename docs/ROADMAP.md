# Lộ trình phát triển PingFootball 4.0

Nền tảng hiện tại là điểm xuất phát. B1–B6 là mục tiêu phát triển, không phải checklist đã hoàn thành. Bắt đầu từ B1; trước mỗi thay đổi đo cơ chế đang có, sau đó thay đổi nhỏ và nghiệm thu riêng. Không reset behavior ID.

## B1. Physics + Rules + Condition

| ID | Phạm vi | Điều kiện nghiệm thu |
|---|---|---|
| PHY-01 | Đơn vị, hệ tọa độ, thời gian | Kích thước/biên sân, frame tấn công, đơn vị, priority, canonical math; mirror/dt/invariant. |
| PHY-02 | Vận động cầu thủ | Thống nhất gia tốc, giảm tốc, đổi hướng, reaction, tốc độ hiệu dụng; distance/time budget. |
| PHY-03 | Thể lực, chấn thương, tâm lý | Tích phân tải đúng một lần, hai bể năng lượng, recovery, injury exposure. |
| PHY-04 | Bóng 3D | Lăn/bay/nảy, drag, gravity, biên và khung thành theo trajectory. |
| PHY-05 | Cầu thủ–bóng | Tách launch/flight/contact/control, race đối xứng, sai số kỹ thuật, cập nhật dự báo khi motion đổi. |
| PHY-06 | Cầu thủ–cầu thủ | Hình học tiếp xúc, tì đè, che chắn, tắc/cản, va chạm thân thể, hụt bóng; tách facts và foul. |
| PHY-07 | Bóng hai | Hướng/động lượng bật ra, nhiều người tranh, xét boundary trước contact; không ép 50/50. |
| PHY-08 | Không chiến | Nhảy, reach, độ cao, heading/clearance, contact, luật. |
| PHY-09 | Thủ môn | Vị trí, lao ra, reach/reaction, bắt/đẩy/đấm, one-on-one, phân phối. |
| PHY-10 | Bóng chết và thể thức | Restart hợp lệ, advantage, thẻ, bù giờ, hiệp phụ/luân lưu. |
| PHY-11 | Mặt sân/thời tiết | Tác động cơ học có căn cứ; không tính penalty hai lần. |
| PHY-12 | Tích hợp | Invariants, audit 54, mirror/sức mạnh/luật/chiến thuật, benchmark và rollback. |

RULE-01 ruleset/restart; RULE-02 offside/advantage; RULE-03 foul/card/penalty theo facts; RULE-04 period/added time/shootout. COND-01 tải vận động; COND-02 injury exposure; COND-03 tâm lý.

Điều tra, không coi là số đo hiện tại: ít phạm lỗi (opportunity → commit → reached contact → body contact → foul → stoppage), bóng sống dài (phân rã restart và thời gian), vận động quá nhiều (target churn/đuổi bóng so với mô hình motion), cách biệt sức mạnh phi tuyến (gradient/factorial skill/fatigue/confidence/utility). Đo lại trên nền tảng trước kết luận. Cấm cap tỷ số hoặc bù đội yếu. Sửa movement trước khi chỉnh stamina nếu sai do chuyển động.

B1.1 (PHY-01) dùng [WORLD_CONTRACTS](WORLD_CONTRACTS.md) làm hợp đồng chi tiết. Thứ tự: B1.1.1 audit, B1.1.2 spatial, B1.1.3 numeric, B1.1.4 time, B1.1.5 invariants, B1.1.6 acceptance.

Bước đầu tiên: PHY-01 inventory các đơn vị và frame trong match_contracts, match_physics, match_rules, engine clock/scheduler; lập bảng caller, invariant, test mirror/dt. Không đổi hành vi trong commit inventory; nếu phát hiện bug cần sửa thành commit B riêng.

## B2. Player Intelligence

| ID | Mục tiêu | Điều kiện |
|---|---|---|
| PI-01 | Observation hữu hạn | Đóng trường tương thích, field-access audit, hidden-world equivalence. |
| PI-02 | Nhận thức thời gian | Độ trễ, uncertainty, dự báo không đọc RNG outcome. |
| PI-03 | Utility chung | Chuyền/sút/dẫn/giữ/phá trên cùng lợi ích, rủi ro và chi phí. |
| PI-04 | On-ball/timing | Chuẩn bị, một chạm, re-aim; không trùng tích phân dt. |
| PI-05 | Chạy chỗ/chuyền khoảng trống | Dự báo hướng chạy, chuyền đón, bẫy việt vị, góc hỗ trợ. |
| PI-06 | Phòng ngự | Tắc/kiềm chế, geometry, thẻ, cover, threat trên Physics mới. |
| PI-07 | Quyết định thủ môn | Vị trí, lao ra/giữ, bắt/đẩy, phân phối; tách thực thi. |
| PI-08 | Memory/personality | Decay, học từ quan sát, không bonus toàn tri. |
| PI-09 | Tích hợp | Isolated tests, forecast calibration, audit 54 và paired population. |

## B3. Match Tactics

Hoàn thiện In Possession, Out of Possession, Attacking Transition, Defensive Transition; role/duty, pressing/screen/cover, chiều rộng/độ sâu, overload, hỗ trợ, bóng chết, độ trễ chỉ đạo, thiếu người. Kiểm thử ma trận High Press/Low Block, Mid Block/Direct Play, tempo/risk, biên/trung lộ và mirror; đo riêng hai đội. Không ép cân bằng kết quả.

## B4. Coach Intelligence

Đưa kế hoạch đội hình và bóng chết hiện còn ở LabTacticalPresets.kt sang PreMatchPlan Rust khi thiết kế đủ. Tách proposal khỏi commit; HLV và người chơi dùng cùng MatchCommand/validation. Xử lý thay người, chấn thương/thẻ đỏ, time management; chống oscillation, cấm đọc tương lai.

## B5. Commentary, Audio, Playback

Narrative từ event đã commit, template EN/VI đúng evidence, seed biến thể độc lập, FIFO mọi rate, ambient/SFX theo context. Nếu phê duyệt speculative lookahead: rollback/re-simulate từ committed checkpoint, chèn lệnh tại timestamp hợp lệ, thay buffer theo generation, không phát cue sai hoặc hai lần. Chốt UX backlog, pause dialog và device matrix trước tích hợp.

## B6. Nghiệm thu

L0–L4 đầy đủ, L5 thực trên APK đúng hash, compatibility/behavior IDs/ruleset/dataset provenance, benchmark và limitations. Không tuyên bố release khi thiếu ma trận thiết bị. Chỉ công bố thay đổi đã có bằng chứng.

## Điều kiện chuyển hạng mục

Mỗi mục: baseline + giả thuyết + test cô lập + dữ liệu paired + đánh giá sai khác và rollback. R/D giữ exact parity; B phải giải thích mọi sai khác có chủ đích. Chốt quyết định mở liên quan trước triển khai, không dùng việc dọn tài liệu thay cho nghiệm thu.
