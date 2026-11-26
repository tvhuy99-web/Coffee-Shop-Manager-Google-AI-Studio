# B1.2 — Vận động cầu thủ

Hợp đồng đích, kế hoạch triển khai và bằng chứng hiện hành; không phải chứng nhận mọi đường gameplay đã chuyển solver. Giữ các thuộc tính quan trọng, không thêm trạng thái động nếu chưa cần. B1.1 đã PASS 19/19 trên 9cafc4d782c4fc425e1d39922237bfd9b668e4c9, run 45778239669. Tất cả thay đổi B1.2 tiếp tục trên feature/b1-1-world-contracts.

## B1.2.1 — Inventory và baseline

| Caller / đường thực thi | Cơ chế hiện hành | Hướng xử lý |
|---|---|---|
| engine/v4/movement.rs: player_max_speed_mps | Pace + Acceleration cùng tăng vmax, hai energy pool trộn | Capability tách vmax và gia tốc |
| movement.rs: advance_off_ball_movement_with_report | PI/Tactics chọn target từ snapshot; chạy thẳng vmax*dt, velocity trung bình, facing đổi tức thời | Giữ target policy trước; chuyển integration sang solver chung |
| ball_play.rs: pass_reach_candidate / build_defender_candidates | Hai profile khác nhau về speed, reaction, reach; contact path có coast và ramp | B1.2.4 dùng cùng capability và trajectory, bảo toàn reach/height riêng của contact |
| match_physics/lib.rs: PassReachCandidate, PassContactMotion, find_pass_contact, find_carry_contact, find_grounded_ball_recovery, find_moving_ball_recovery | Absolute-time path cho pass/carry/loose; early interaction còn dùng reach đơn giản | B1.2.4: forecast dùng cùng path với commit, không đổi winner trước population evidence |
| pi_adapter/pass_forecast.rs | Đối thủ chỉ thấy speed quan sát và profile mặc định | Giữ giới hạn thông tin; dùng solver với capability ước lượng, không đọc thuộc tính ẩn |
| session_parts/01_world.rs | Off-ball move trước rồi ghi đè committed carry/flight/challenger; kiểm chord budget; velocity trung bình | Chọn đúng một motion plan cho mỗi player trước tích phân; terminal velocity và actual path length |
| session_parts/01_world.rs, 05_condition.rs | Dead-ball đi theo walk speed; fatigue dùng 0.35/0.15 m/s ước lượng, live dùng chord, sprint count theo interval | B1.2.5–6: shared solver, workload từ path thật; không đổi condition model tại đây |
| ball_play.rs: execute_dribble_duel_impl | Đường dribble legacy sửa pos/fatigue trực tiếp | Kiểm tra caller production vs compatibility; không coi mã tồn tại là chắc chắn live |
| shooting.rs: rebound chase | ETA distance/speed, sửa pos trong resolver | B1.2.4–5: kiểm tra thời gian thật, không dịch chuyển thêm trong resolve |
| set_pieces.rs: aerial contest | Đặt pos tại delivery spot | B1.2.5 / B1.3–4: rà soát đường bay, tiếp cận, restart |
| session_parts/04_period.rs, session.rs | Kickoff reset theo vị trí được luật cho phép | Giữ reset hành chính phân biệt với live locomotion |
| goalkeeper.rs, player intelligence, match tactics | Policy dùng vị trí/tốc độ/reach | B1.2.4–5 chuyển forecast có giới hạn thông tin; không chỉnh quyết định ngoài scope |

Baseline đo lại từ CI job 139767073210, log V4_REALISM_BASELINE: 1.000 trận synthetic, Home level 14/Away level 12, 2.473 bàn, 25.976 sút, 71.806% pass, 1.857 foul, 81.025 phút bóng sống, 13.416 phút bóng chết, 5178.157 DES steps/trận, 14.879 matches/s trên runner đó; Home thắng 81.3%, hòa 14.5%, Away 4.2%. Đây là đối chứng, không phải mục tiêu tối ưu. Report 128 trận cũng PASS (2.477 bàn, 25.766 sút, 71.800% pass). Log tổng hợp chưa có phân bố distance/sprint theo vị trí; không tái sử dụng con số 14.6 km cũ như số đo hiện tại. B1.2.6–7 bổ sung đo paired distance, speed band, acceleration, target churn, live/dead, GK/outfield theo cohort; giữ seed và fixture. Host benchmark L1 cũng đã PASS, không suy ra tốc độ Android. Baseline log: https://github.com/tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio/actions/runs/45778239669/job/139767073210 .

## B1.2.2 — Movement capability

Capability là giá trị suy ra, không phải chỉ số cầu thủ lưu thêm. Pace chỉ vào max speed; Acceleration chỉ vào forward acceleration; Agility vào braking/lateral steering/body turn. Aerobic ảnh hưởng vmax, Anaerobic ảnh hưởng khả năng burst và phanh/steering; giữ hai pool, không nhân fatigue thêm lần nữa trong solver. Balance giữ vai trò ổn định kỹ thuật/tiếp xúc; Strength, Stamina, Natural Fitness, Jumping Reach giữ nguyên thuộc tính và cơ chế hiện tại, không gộp hay thêm penalty tùy ý. Reaction là thời gian intent có hiệu lực do caller cung cấp, không tự dùng hidden world hoặc suy ra lần hai từ anticipation. Injury restrictions do engine/condition cung cấp khi tích hợp, không tự tạo penalty chấn thương.

Các hệ số đầu tiên của prototype chỉ là giả thuyết cơ học, chưa hiệu chỉnh với tracking. Đầu vào hữu hạn và đúng miền, reject invalid thay vì clamp attribute/energy sai. Không đổi 54 attributes, checkpoint, JNI hay behavior ID ở bước cô lập.

## B1.2.3 — Solver

Một module match_physics/movement.rs, không thêm crate. MotionState (position, velocity, facing), MovementIntent (target, intensity, optional facing, reaction delay), MovementCapability, MotionPlan/MotionResult. Intent/capability không đổi trong cùng plan. Pure, deterministic, không RNG, không access engine. Tích phân vận tốc tuyến tính theo các đoạn local 50 ms neo từ origin plan; reaction boundary tách chính xác. Không có global tick. Cùng plan lấy mẫu ở timestamp bất kỳ không phụ thuộc thứ tự query; không replan chỉ vì polling. Đây chưa phải chứng minh split/replan toàn engine.

Target speed dựa trên quãng đường phanh; longitudinal acceleration/braking và lateral acceleration có giới hạn riêng; khi phải đảo hướng, phanh tới zero forward component trước khi tăng tốc ngược. Không teleport/clamp position để đến target. Facing tách với velocity, rotate theo bound, có thể nhìn hướng khác với chuyển động. Trong reaction, giữ velocity/facing cũ. Nếu initial speed vượt limit do capability thay đổi, phanh trong giới hạn, không snap. Local solver không tự quyết định luật sân, va chạm, contact reach hoặc ball control; caller sẽ xử lý boundary/reforecast.

MotionResult trả vị trí, terminal velocity, facing và tích lũy path length (tích phân norm vận tốc bằng Simpson theo đoạn), acceleration load (tổng norm delta velocity); chưa tính sprint episode hoặc thay condition. Giới hạn horizon kỹ thuật 60 s/request, reject thay vì clamp; caller phải chia/replan rõ, không áp cap gameplay. B1.2.4 tối ưu/cache path cho search nhiều lần và chọn analytic fast path nếu profiling cần. 50 ms là độ phân giải prototype, cần convergence và benchmark trước production; không giả định dt-split nếu replan hoặc condition đổi.

## Trình tự còn lại

| Mục | Gate |
|---|---|
| B1.2.1 | Inventory các đường chính + baseline CI log; distance/sprint telemetry còn thiếu |
| B1.2.2 | Test độc lập Pace/Acceleration/Agility, energy, invalid |
| B1.2.3 | Test acceleration, brake, turn, reaction, facing, intensity, overspeed, mirror, query split, finite, path |
| B1.2.4 | Reachability dùng cùng solver, continuous contact order và forecast hữu hạn |
| B1.2.5 | Tích hợp một path/player/interval, live/dead/restart, B commit riêng |
| B1.2.6 | Motion accounting, distance/sprint/condition, invariants |
| B1.2.7 | Paired >=1000, cohorts và hiệu chỉnh từ nguồn thật, benchmark |
| B1.2.8 | Audit 54, cross-mode, checkpoint, CI L0–L4, reviewed behavior/reference change |

B1.2.1–3 không gọi solver mới từ production. Exact parity bắt buộc; không thay golden/fixtures hoặc workflow. Chưa tuyên bố solver calibrated. B1.2.4–8 mới tạo behavior change khi wire runtime, cần evidence và behavior ID phù hợp. Giữ 54 thuộc tính, không quota thống kê. Rollback từng commit, không force push.
