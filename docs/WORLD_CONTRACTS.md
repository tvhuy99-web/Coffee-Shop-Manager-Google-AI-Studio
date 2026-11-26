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

## B1.1.4 — Hợp đồng thời gian và nhân quả

- Nguồn thời gian duy nhất là DesClockState, ms nguyên. Scheduler sắp xếp (time_ms, priority, insertion_sequence); sequence không được tràn và tái sử dụng. Cùng timestamp: 10 PhysicsAndBoundary, 20 RuleAndOfficiating, 30 BallStateTransition, 40 DeadBallAndSubstitution, 50 TacticalAndCommand, 60 ActionDecision, 70 PeriodBoundary. FIFO trong cùng priority. Đây là thứ tự các sự kiện đã nằm trong queue, không phải quyền đảo ngược quan hệ nhân quả của sự kiện mới sinh.
- Event bị hủy theo generation không tiến thời gian hoặc watchdog. Persistent sống qua đổi hiệp. Cursor không đi ngược; tại commit/restore phải bằng sim_time_ms. Không thêm clock hay trường serialize.
- Với mỗi khoảng dt>0, advance_ms cập nhật mọi bộ đếm một cách nguyên tử, sau đó world integrator được gọi một lần trước dispatch. dt=0 không tích phân. sim_time_ms = period_started_at_ms + period_elapsed_ms; live + dead = sim; home + away possession = live (định nghĩa accounting hiện tại, không khẳng định mọi thời điểm có owner thực).
- Bù giờ: compensable lost, announced và additional là counter khác nhau. Không đặt cap nghiệp vụ; watchdog chỉ là lỗi kỹ thuật. Đổi hiệp reset counter cục bộ, giữ thời gian tích lũy. UI polling không làm phát sinh dt. Không đổi thứ tự RNG.
- Test mới: split integer time với live/dead và dt=0, overflow không mutate; đủ 7 priority và FIFO sau serialize/restore; canceled events không tiêu thụ watchdog; sequence exhaustion.
- Giới hạn: motion path commit đã có test split cho carry; không suy ra toàn bộ PI, movement hay fatigue có dt-split invariance. Off-ball target/RNG và điều kiện theo interval cần B1.2/B1.7. Resolver point-ball vẫn dùng nearest-ms; hai contact liên tục cùng ms cần chọn thứ tự trên thời gian liên tục trước khi ceil và schedule. Không đổi priority/rounding production ở B1.1 để che khác biệt; tích hợp và đo paired tại B1.3/B1.4. Không thêm clock nano giây hay tick toàn cục.

## B1.1.5 — Bất biến và phạm vi thực thi

| ID hiện có | Kiểm tra | Vị trí |
|---|---|---|
| INV-01/04 | Loose/InFlight không owner; actor đủ điều kiện, controlled action đúng owner; full diagnostic kiểm owner | transition.rs, action paths, commit/restore |
| INV-02 | Finite ball/player kinematics, target, facing; movement budget | structural commit/restore, world integration |
| INV-03 | Thời gian/step/event không lùi; period accounting, scheduler cursor; generation/timestamp hợp lệ | per-event, public transaction, restore |
| INV-05 | Goal/own-goal delta đối chiếu sự kiện và đội | event-local audit |
| INV-07/08 | Profile và roster identity, fatigue 0..100, distance không âm, command journal | public transaction, restore |
| INV-10 | Live/dead/possession accounting | per-event, commit, restore |
| INV-11/12 | Retained event order, kết thúc một lần, không sửa kết quả đóng | public transaction, restore |

Diagnostic read-only được đồng bộ giới hạn energy 0..100 và kiểm finite facing, profile/ID, uncontrolled ownership. Không clamp hoặc repair. Kiểm tra full diagnostic không được gọi thay structural gate tại mọi event vì chi phí và ngữ nghĩa chuyển tiếp; không tự nhận mọi invariant đã được chứng minh. Thêm negative tests cho period/scheduler corruption với checksum tính lại, transaction rollback cả RNG, diagnostic không mutate. Scheduler restore từ chối generation tương lai, timestamp âm, persistent generation sai, nhưng chấp nhận event cũ đã hủy.

Chưa tăng behavior ID vì các thay đổi nhằm bảo toàn trận hợp lệ; thay đổi có chủ đích là từ chối trạng thái hỏng/tràn số. Đây là hardening, không phải sửa luật bóng đá. Rủi ro: checkpoint cũ có clock/generation không nhất quán sẽ bị từ chối rõ thay vì restore; replay hợp lệ phải giữ nguyên. Không thay schema hoặc ABI.

## B1.1.6 — Nghiệm thu

Baseline đầu B1: 3eead20b (CI 45774269908, 19/19 PASS). B1.1.1–3: 558139bf (CI 45777132698, completed success). Candidate B1.1.4–6 phải chạy toàn bộ workflow L0–L4; chưa ghi PASS trước khi có kết quả.

- L0 static/contract và lock/DAG.
- L1 workspace all-targets, gồm toàn bộ unit tests b11_, isolated mechanisms và host transaction benchmark.
- L2: 1.396 trận golden, 1.384 cross-mode, checkpoint markers. Không đổi reference, fixture hoặc tolerance.
- L3: audit 54 thuộc tính (904 simulations) và diagnostic population.
- L4: Kotlin, commentary, ARM64 APK và JNI exports. L5 không nằm trong đợt này.
- So sánh candidate với baseline; report first divergence nếu đỏ; không đổi golden. Rollback bằng revert commit tương ứng; không force-push. Không merge khi còn queued/running/failed.

Điều kiện ra B1.1: các test và gate trên xanh, giới hạn chuyển sang B1.2/B1.3 đã ghi rõ. Các helper toàn quả bóng và ceil là hợp đồng đã có test riêng, chưa phải production ball resolver. Không tuyên bố đã hoàn tất ball dynamics, unified locomotion hay full dt invariance.
