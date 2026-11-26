# Hợp đồng thế giới (B1.1)

Tài liệu chuẩn cho không gian và số học của Physics, Rules, Condition, PI và Tactics. Đọc cùng ARCHITECTURE và TESTING. Đối chứng đầu vào: 3eead20b597e13f4c81910ec949683fd5ab652d8, CI 45774269908 (19/19). Đây không phải chứng nhận CI của thay đổi mới.

## B1.1.1 — Kiểm toán các đường thực thi chính

| Nguồn / caller | Hợp đồng hiện tại | Quyết định |
|---|---|---|
| match_contracts/spatial.rs → engine contracts/spatial.rs, v4/spatial.rs | Vec2/Vec3 f32, sân 105×68, Home hướng +X; Away quay 180° | Giữ tọa độ và re-export; dùng chung phép đổi frame, không clamp khi đổi frame |
| spatial.rs → state.rs, movement.rs | slot width/depth 0–100, chuyển ra mét | Clamp đầu vào slot như trước; không dùng slot làm tọa độ vật lý |
| spatial.rs → ball_play.rs, shooting.rs, set_pieces.rs | attacking progress, goal center, penalty area, goal/line geometry | Giữ semantics hiện tại; phân biệt progress có clamp với tọa độ không clamp |
| match_physics/lib.rs → v4/interaction.rs (re-export), ball_play.rs, shooting.rs | ray point boundary, goal mouth point, trajectory, nearest-ms, local contact search | Chưa xét toàn quả bóng ở các primitive này; không đổi kết quả trong refactor. Chuẩn bị geometry toàn quả bóng cho B1.3 |
| match_contracts/units.rs → clock.rs, match_rules/lib.rs | i64 ms, 45/15 phút; hằng số trùng | Một nguồn hằng số, giữ tên tương thích |
| match_physics/lib.rs, ball_play.rs, shooting.rs, session_parts/01_world.rs | dt f32 sec, làm tròn thời điểm bằng round | Dùng helper cùng biểu thức và thứ tự phép toán; không đổi nearest thành ceil trong production |
| match_rules/lib.rs | thời gian luật và phản ứng; round rồi bound | Giữ policy làm tròn riêng theo luật; không thay hàng loạt |
| clock.rs, scheduler.rs, session.rs, session_parts/01_world.rs | đồng hồ monotonic, 7 priority, tích phân theo event, pending motion override | B1.1.4 kiểm toán tie/continuous sub-ms và dt split; không đổi scheduler ở đây |
| transition.rs | finite, time/order, roster, control owner, movement budget | B1.1.5 đánh giá coverage; không thêm validator trùng |
| player_condition/lib.rs | fatigue theo interval, energy hiện hành | Không đổi mô hình thể trạng trong B1.1.1–3 |

Đây là kiểm toán theo các đường chính đã đọc, không phải chứng minh mọi literal/caller trong repository đều đã chuẩn hóa. Các hằng số chiến thuật (khoảng cách chạy, vùng mục tiêu), xG-millis, probability và các chuyển đổi ms→giây của UI không được thay bằng hằng số hình học một cách máy móc.

## B1.1.2 — Không gian

- World frame cố định: X 0–105 m, Y 0–68 m; Home tấn công +X, Away -X. Z hiện là độ cao theo mô hình point-ball, z=0 khi ground. Frame tấn công: tiến về khung thành đối phương theo +X, Away quay cả X và Y; đổi frame không clamp, không teleport. Hướng tấn công logic cố định qua các hiệp; môi trường có hướng vật lý sẽ phải xét đổi đầu sân khi triển khai.
- Vector vận tốc/direction quay bằng đổi dấu X,Y đối với Away, không dịch chuyển; tọa độ điểm quay quanh tâm sân. Facing radian tính từ +X, dương hướng +Y. Không tạo trường state mới.
- Sân 105×68 là cấu hình chuẩn mô phỏng, không tuyên bố mọi sân ngoài đời có kích thước này. Các hằng số penalty/goal/center circle trong spatial.rs là nguồn dùng chung.
- Đường biên cũ: ray point và goal-mouth point vẫn hoạt động như baseline. Hợp đồng đích: chỉ xác nhận bóng ra ngoài khi toàn bộ hình cầu vượt mép ngoài vạch; tiếp xúc vạch không đủ. Geometry toàn quả bóng dùng tâm hình cầu vật lý và bán kính dương, không dùng trực tiếp z=0 của point-ball. Mặt phẳng biên chuẩn là mép ngoài vạch; vùng cửa gôn sử dụng mép trong cột và mặt dưới xà. Va chạm cột/xà và quỹ đạo là B1.3; goal validity do Rules quyết định. Hàm hình học mới không thay thế resolver hiện tại cho đến khi có behavior commit và population evidence.
- Không ép đối xứng bit-exact qua phép trừ f32 hai lần; mirror geometry test dùng giá trị biểu diễn chính xác hoặc sai số kiểm chứng theo phép toán. Không dùng dung sai để ép outcome.

## B1.1.3 — Số học

| Đại lượng | Đơn vị / kiểu |
|---|---|
| Vị trí, khoảng cách | m, f32 |
| Vận tốc, gia tốc | m/s, m/s², f32 |
| Góc, tốc độ xoay | rad, rad/s, f32 |
| Timestamp / duration | ms, i64, không âm tại boundary hợp lệ |
| Liên tục trong solver | seconds, f32 hoặc f64 khi cần |
| Thuộc tính | u8, 1..20 |
| Probability | f32, 0..1 |
| Energy | f32, theo miền của condition; không thay schema ở đây |
| Hash tọa độ | quantize_mm, hiện hành |

- Chuyển ms→s giữ cast trước divide; nearest f32 seconds→ms giữ multiply trước round và cast, gồm hành vi cast bão hòa hiện hành. Helper tương thích không phải validation đầu vào. Không dùng nó cho dữ liệu ngoài chưa kiểm tra hữu hạn và miền hợp lệ.
- Hợp đồng đích cho event vật lý: so sánh thời điểm liên tục trước khi lượng tử hóa; nếu dùng ms nguyên thì ceil offset hữu hạn không âm, từ chối overflow. Đây là policy cho tích hợp sau, không thay nearest trong resolver hiện hành. Không dùng ceil cho thời lượng luật hoặc fingerprint.
- Không đổi float operation order, RNG, hash, schema hay behavior ID trong R/T. Dung sai geometry chỉ dựa trên sai số biểu diễn, không quota gameplay. Test numeric phủ zero, split đơn giản, near-half, invalid/overflow, và tương thích biểu thức cũ.

## Nghiệm thu và phần còn lại

B1.1.1: inventory các đường chính. B1.1.2–3: contract/helper và test cô lập, migration caller không đổi biểu thức. L0–L4 và exact golden vẫn bắt buộc trước công nhận parity. B1.1.4–6 chưa thuộc phạm vi: time/scheduler causality, dt/invariant coverage, acceptance tổng thể; B1.2/3 chưa đổi movement/ball solver. Nếu test chỉ ra lỗi luật/vật lý, sửa bằng commit B riêng có behavior ID, paired population và review golden; không nới gate.
