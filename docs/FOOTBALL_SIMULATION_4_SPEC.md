HỆ SINH THÁI MÔ PHỎNG BÓNG ĐÁ 4.0
HIẾN CHƯƠNG KỸ THUẬT, KIẾN TRÚC CHUẨN VÀ KẾ HOẠCH TÁI CẤU TRÚC
Revision 3 (Bản chuẩn cuối cùng sau kiểm toán mã nguồn) | 25/11/2026 | Dự án PingFootball
Trạng thái: Bản đặc tả chuẩn hoàn chỉnh đã đối chiếu trực tiếp với mã nguồn thực tế, sẵn sàng đưa vào repository sau khi chủ dự án chốt các ADR tại Mục 14. Chưa phải bằng chứng triển khai hoặc nghiệm thu.
Tên tệp chuẩn trong repository: docs/FOOTBALL_SIMULATION_4_SPEC.md
Mốc mã nguồn tham chiếu: c2c392d8b3ccc57985e87337e8994fd7033328ee (PI 4.0.28).
Phạm vi: Rust Match Engine, Player Intelligence, Coach Intelligence, chiến thuật, vật lý, luật, thể lực/tâm lý, thống kê và tầng trình diễn Android.
1. Thẩm quyền tài liệu, phạm vi và tầm nhìn
1.1. Single Canonical Specification
Sau khi chủ dự án phê duyệt và đưa vào repository, văn bản này là nguồn quy định kiến trúc duy nhất (Single Canonical Specification) cho Hệ sinh thái Mô phỏng Bóng đá 4.0. Các tài liệu cũ không còn quyền quy định kiến trúc khi mâu thuẫn với văn bản này. Tuy nhiên, bằng chứng lịch sử, quyết định kỹ thuật, fixture, dữ liệu thí nghiệm và báo cáo lỗi phải được bảo tồn, có ghi rõ trạng thái hết hiệu lực về mặt quy định. Không được sử dụng lý do «đã có tài liệu chuẩn» để xóa khả năng truy nguyên một thay đổi.
Từ khóa trong đặc tả:
• BẮT BUỘC / CẤM: điều kiện hợp đồng; vi phạm là lý do chặn nghiệm thu.
• NÊN: mặc định kỹ thuật, được phép thay đổi nếu có quyết định kiến trúc (ADR) và bằng chứng.
• ĐỊNH HƯỚNG: mục tiêu tương lai; không đồng nghĩa đã có tính năng, không yêu cầu tạo module rỗng.
• CHỜ XÁC MINH: chưa đủ bằng chứng; không được chuyển trạng thái thành đạt chỉ vì biên dịch được hoặc số liệu trung bình đẹp.
Các đoạn Rust, cây thư mục và sơ đồ dưới đây là đặc tả giao diện đích, không phải mã nguồn đã biên dịch. Chúng được hiện thực hóa theo từng chặng và phải qua các cổng kiểm thử tương ứng. Những lựa chọn có ảnh hưởng đến hành vi, tương thích hoặc UX chưa được chủ dự án phê duyệt được liệt kê ở Mục 14.
1.2. Tầm nhìn
Xây dựng một thế giới bóng đá có quy luật vật lý, luật thi đấu và giới hạn năng lực nhất quán; trong thế giới ấy, cầu thủ và HLV nhận thức, lập kế hoạch, quyết định, thực thi, thất bại và thích nghi. Kết quả trận đấu là hệ quả của các cơ chế đó, không phải đầu vào để ép hành vi.
Hệ thống dùng Discrete Event Simulation (DES) với đồng hồ nguyên mili-giây i64; mô phỏng đủ hai hiệp 45 phút cộng bù giờ, hiệp phụ và luân lưu khi thể thức yêu cầu. Không rút gọn 90 phút thành số lượt hành động quy ước. Thời gian chuẩn bị kỹ thuật, di chuyển, bóng bay, xử lý luật, chấn thương và bóng chết phải có ý nghĩa mô phỏng. Android trình diễn ở 1.0x, 1.25x, 1.5x, 2.0x, 3.0x, không điều khiển kết quả gameplay. Nghỉ giữa hiệp không bắt người chơi chờ 15 phút ngoài đời; đó là trạng thái chuyển tiếp riêng, cho phép quản lý đội hình và chiến thuật, không tính 15 phút nghỉ vào đồng hồ thi đấu.
Bốn trụ cột:
1. Thực tại: Physics, Rules, Condition mô tả cầu thủ, bóng và điều kiện thi đấu có thể xảy ra những gì.
2. Trí tuệ: Player Intelligence, Tactics, Coach Intelligence mô tả chủ thể muốn làm gì và phối hợp ra sao, trong giới hạn nhận thức.
3. Điều phối: Match Engine sở hữu trạng thái toàn trận, thời gian và quy trình xác nhận sự kiện.
4. Quan sát: Telemetry, Android Playback, Commentary, Audio phản ánh những gì xảy ra; không điều khiển xác suất gameplay.
1.3. Phạm vi 4.0
Trong phạm vi: tạo MatchInput; kế hoạch trước trận; đội hình chính/dự bị, vai trò; điều kiện sân/thời tiết; 54 thuộc tính và trạng thái hiệu dụng; mô phỏng thời gian thi đấu; lệnh tương tác; thay người; tình huống bóng sống/chết; hiệp phụ và luân lưu theo thể thức; trạng thái nghỉ giữa hiệp; kết thúc trận; MatchResult; thống kê, chấm điểm; checkpoint; seed replay và event replay nếu đã ghi đủ dữ liệu; phát lại, TTS, âm thanh.
Ngoài phạm vi: quản lý CLB dài hạn, tài chính, hợp đồng, chuyển nhượng, giải đấu nhiều mùa, đào tạo trẻ, lịch thi đấu liên giải, phát triển thuộc tính dài hạn. Các hệ thống ấy cung cấp dữ liệu đầu vào được phép qua MatchInput và tiêu thụ kết quả qua MatchResult. Không tạo phụ thuộc từ Match Engine vào cơ sở dữ liệu sự nghiệp, UI CLB, mạng hoặc hệ thống thanh toán. Có thể thiết kế kiểu mở rộng hợp đồng nhưng không tạo crate, API giả hoặc dữ liệu giả để «giữ chỗ».
1.4. Điều không được suy diễn
Một trận có thống kê gần bóng đá thật không chứng minh vật lý đúng. Một AI thắng nhiều không chứng minh quyết định tốt. Một test đơn vị PASS không chứng minh toàn bộ trận đúng. Exact parity chỉ chứng minh bản tái cấu trúc giữ hành vi mốc đối chứng, kể cả những hạn chế đã biết; không chứng minh hành vi đó chân thực. Kiến trúc đích chưa đồng nghĩa mã nguồn hiện tại đã đạt đích.
2. Mốc thực tế trước tái cấu trúc (Đã kiểm toán mã nguồn)
2.1. Hai mốc cần bảo tồn và kết quả kiểm toán c2c392d8
Mốc PI 4.0.27: commit 1d7bf1b43a4384dff8b8ffbcb4953d18933e9393, workflow DES 45520666910, audit thuộc tính 45520666555. Rust toàn bộ được báo 185 PASS, 0 FAIL, 8 ignored; APK ARM64 thành công; P1 kiểm tra tích hợp tĩnh 17/22, chưa đạt.
Mốc PI 4.0.28: commit c2c392d8b3ccc57985e87337e8994fd7033328ee, workflow DES 45527962031. 1.000/1.000 trận baseline hoàn thành, không có diagnostics không hợp lệ; 15/16 job DES PASS; job on-ball FAIL tại pi4_legacy_api_and_direct_policy_match. Chưa có bằng chứng audit 904 trận trên chính commit 4.0.28. Không công nhận 4.0.28 là release xanh.
Kết quả kiểm toán trực tiếp mã nguồn c2c392d8:
• Quy mô mã nguồn: 37.320 dòng Rust (39 tệp ở root src/ gồm 34 ứng viên legacy ~14.651 dòng và 5 tệp chia sẻ/biên; 28 tệp v4/ ~20.717 dòng, riêng v4/session.rs 4.221 dòng và v4/tests.rs 3.727 dòng; 13 tệp player_intelligence/ ~1.952 dòng) và 30 tệp Kotlin Android (tầng lab/ 8.245 dòng, 0 tệp Java trong app/).
• Khóa biên dịch còn thiếu: Repository hiện chưa có rust/Cargo.lock lẫn rust-toolchain.toml; toàn bộ 10 workflow GitHub đều dùng dtolnay/rust-toolchain@stable trôi nổi.
• Nguyên nhân đỏ CI hàng loạt ở 4.0.28: v4/tests.rs:2691 trong test pi4_legacy_api_and_direct_policy_match đang hard-code assert_eq!(pi::PI_MODULE_VERSION, "4.0.27"); trong khi player_intelligence/version.rs là "4.0.28". Do hầu hết workflow (build-apk.yml, validate-des-repair.yml, validate-v4-p1.yml, validate-port.yml) đều chạy cargo test --all-targets, một dòng assert này làm fail dây chuyền các workflow.
• Phân loại 5/22 check FAIL của tools/verify_v4_p1_234.py:
• 3 check lỗi thời theo mô hình beat cũ (trước DES advanceUntilMs): maxSteps = if (V4) 1 else 8, presentedV4Beat != simulatedV4Beat, commandDialogActive + main.removeCallbacks(tick).
• 2 check có giá trị nghiệp vụ cần xác minh/sửa ở A0: hỗ trợ chọn thay nhiều người cùng lúc (setMultiChoiceItems / submitSubstitutions( trong LabReferenceMatchActivity.kt) và cơ chế chống ghi đè archive trận khác (if (previous == record) return trong LabGlobalSeedReplayStore.kt).
• Tài nguyên âm thanh và bình luận (Đã xác minh): Đủ 48/48 tệp .ogg trong app/src/main/assets/mod_audio/core/, sha256sum -c AUDIO_SHA256_MANIFEST.txt PASS 100%, có app/src/main/assets/AUDIO_LICENSES.md, và tools/verify_lab_v2.py (101/101 PASS) đã xác minh 48 checksum cùng 895 mẫu bình luận EN/VI đồng nhất token.
• Phụ thuộc v4/ vào root legacy: v4/ chỉ import đúng 3 module ở root (crate::clock, crate::fingerprints, crate::rng), hoàn toàn không phụ thuộc 34 tệp logic V3 còn lại. Tuy nhiên tools/verify_lab_v2.py (được gọi trong build-apk.yml và validate-port.yml) đang assert cứng sự tồn tại của 11 tệp legacy ở root.
Chỉ số (trung bình mỗi trận baseline)
PI 4.0.27
PI 4.0.28
Bàn thắng
2,533
2,477
Cú sút
26,305
25,981
Đường chuyền
1.211,802
1.218,264
Chuyền chính xác
71,77%
71,80%
Bóng sống (phút)
80,43
81,03
Bóng chết (phút)
chưa ghi lại ở đây
13,42
Va chạm tắc bóng được audit
42,97
24,093
Tắc bóng kiểm soát
3,626
2,232
Tắc bóng tạo bóng tự do
8,065
4,432
Bóng hai về đội tấn công
3,880
1,722
Bóng hai về đội phòng ngự
4,168
2,707
Phạm lỗi dừng trận từ audit tắc bóng
3,72
1,855
Nguồn 4.0.28: job baseline 138915898432, các marker DES_POPULATION_EVIDENCE, PI_TACKLE_CONTROL_SPLIT, PI_TACKLE_CONTACT_AUDIT, PI_TACKLE_SECOND_BALL_PRIOR. Số liệu phạm lỗi ở bảng là nhóm contact audit, không mặc nhiên đồng nghĩa tổng mọi phạm lỗi. Các tỷ lệ thu hồi phải ghi mẫu số; không phân bổ các trường hợp ra biên hoặc chưa giải quyết cho một đội chỉ để tổng đẹp. Tất cả số liệu được làm tròn khi trình bày; đối chiếu dùng bản JSON gốc.
Mẫu chuẩn tổng hợp 14 đối đầu 12 không phải đội hình cầu thủ thật. Nhóm 13 đối đầu 13 và 16 đối đầu 9 ở 4.0.27 đã chỉ ra vấn đề: trận cân bằng ít bàn, trong khi chênh lệch lớn tạo 13–14 bàn và hơn 90 cú sút toàn trận; khoảng 14,6 km vận động ngoài sân trung bình ở baseline. Những số liệu đó là dấu hiệu cần điều tra, không phải kết luận nhân quả.
2.2. Chọn mốc tái cấu trúc
Mặc định khuyến nghị (ADR-001): đóng băng cả 4.0.27 và 4.0.28, chọn HEAD 4.0.28 (c2c392d8) làm mốc hành vi cho tái cấu trúc (vì lỗi CI chính chỉ do 1 dòng version literal ở v4/tests.rs:2691), bổ sung chạy audit 904 trận trên chính 4.0.28 tại A-1/A0. Không tự động revert về 4.0.27; cũng không coi quyết định tắc bóng mới là đã nghiệm thu độ chân thực. Nếu chủ dự án chọn revert về 4.0.27, việc revert phải là commit riêng được phê duyệt và tạo lại baseline. Mốc cuối cùng phải ghi trong hồ sơ A-1 và không thay đổi lén giữa các chặng.
Giữ nguyên các hạn chế đã ghi nhận (tần suất va chạm thấp, bóng sống quá dài, chuyển động chưa thống nhất, AI có đường truy cập dữ liệu rộng, 2 check P1 cần xác minh) trong các commit refactor thuần túy. Sửa chúng ở commit hành vi hoặc tích hợp được phân loại rõ. Tạm dừng tăng PI 4.0.29 trở đi cho tới khi nền tảng đạt cổng Phần A.
3. Hiến chương kỹ thuật: 15 nguyên tắc
ID
Quy định bắt buộc
Bằng chứng chấp nhận
HC-01
Mechanism-First. Cấm nắn tỷ số, số cú sút, lỗi, thẻ, phút bóng sống hoặc tỷ lệ chuyền bằng quota, vòng phản hồi theo mục tiêu thống kê, ưu đãi đội yếu không có nguyên nhân bóng đá. Được hiệu chỉnh tham số có ý nghĩa vật lý/hành vi khi có bằng chứng cơ chế; giới hạn luật, miền số học và watchdog hợp lệ không phải quota thống kê.
Test cô lập, giả thuyết, dữ liệu quan sát và kiểm chuẩn nhiều quần thể.
HC-02
Tách refactor và đổi hành vi. Refactor giữ exact parity gameplay trên mốc khóa; thay đổi quy tắc, xác suất, thứ tự RNG, quyết định hoặc timing là thay đổi hành vi dù dòng code ít.
Diff đầu tiên, fingerprint, checkpoint, phân loại commit.
HC-03
AI nhận thức hữu hạn. PI/Coach chỉ dùng Observation và nguồn ngẫu nhiên được cấp; không đọc state thật, ý định đối phương, seed/luồng RNG của bộ giải kết quả. HLV AI và người chơi cùng đường kiểm tra MatchCommand.
Kiểm tra phụ thuộc, kiểm thử hai thế giới có observation giống nhau, command parity.
HC-04
Rust là nguồn sự thật gameplay. Thời gian DES nguyên ms; polling, tốc độ phát, phân lô và restore không thay đổi gameplay với cùng đầu vào/lịch lệnh. Chia dt khi điều kiện không đổi phải tuân theo hợp đồng số học.
FullEvent/FastBatch, exact continuation, kiểm tra dt và scheduler.
HC-05
Hệ sinh thái 4.x.y; định danh hành vi và schema rõ ràng; không đổi ngầm checkpoint/seed replay. Phiên bản trình diễn không vô hiệu hóa checkpoint lõi.
Manifest, compatibility tests, release note.
HC-06
Evidence-Gated. Đổi Physics/Rules/Condition/Tactics/PI/Coach phải có test cô lập, >=1.000 trận ghép seed, các cohort bổ sung và audit 54 thuộc tính (bộ 904 hiện hành sau khi xác nhận fixture). Static check chỉ là vòng lọc; không thay cargo/build/device.
Báo cáo CI, JSON gốc, test coverage và sai khác giải thích được.
HC-07
Không mã giả, không fallback âm thầm. Chỉ tạo module có hợp đồng và chức năng thật; thay thế legacy phải cập nhật call site/test/CI cùng đợt, hoặc ghi ngoại lệ chuyển đổi có thời hạn.
Kiểm kê phụ thuộc, test lỗi, danh sách ngoại lệ đóng được.
HC-08
Trung thực nghiệm thu. Phân biệt đã viết mã, đã build, đã kiểm thử, đã kiểm chuẩn, đã nghiệm thu thiết bị. Không có log không được tự nhận PASS.
Biên bản nghiệm thu và giới hạn công khai.
HC-09
Engine là chủ sở hữu trạng thái authoritative; module trả kết quả/delta có kiểu; commit nguyên tử, không áp dụng nửa giao dịch.
Invariant, fault injection, test hủy sự kiện, checkpoint.
HC-10
Bảo toàn bất biến vật lý và luật; không teleport, không hai chủ bóng, không thực thi hai lần, không ghi bàn trái luật.
Property test, luật tình huống, runtime debug assertions.
HC-11
Đơn vị, hệ tọa độ, miền giá trị, phép toán, thứ tự tie-break chuẩn hóa. Không dùng epsilon hoặc clamp để che lỗi; dung sai có lý do và được phiên bản hóa.
Unit/geometry tests, test mirror, dt, cross-platform.
HC-12
Tách gameplay khỏi Telemetry và Presentation; đo lường không tác động RNG/decision; UI không tự tạo sự kiện gameplay.
Telemetry on/off parity, presentation-rate parity.
HC-13
Bảo tồn bằng chứng, seed, input, toolchain, hash và ADR; tài liệu cũ không có thẩm quyền nhưng vẫn truy nguyên được.
Baseline manifest, tag bất biến, artifact checksum.
HC-14
Ngân sách CPU, RAM, kích thước, pin, nhiệt, backlog và lookahead có kiểm chứng trên thiết bị mục tiêu. Không lược cơ chế gameplay theo tốc độ phát hoặc thiết bị yếu mà không có hợp đồng mới.
Benchmark tái lập và device matrix.
HC-15
Thay đổi nhỏ, có thể hoàn tác; mỗi chặng có điều kiện vào/ra và rollback; không hợp nhất khi cổng bắt buộc đỏ.
Commit/PR độc lập, review, diff, tag và nhật ký chặng.
Khi HC-02 (parity) xung đột với HC-03 (nhận thức hữu hạn) trong Phần A, dùng adapter tương thích và trait port có ghi ngoại lệ tạm thời, rồi sửa hành vi ở Phần B. Không coi ngoại lệ tạm thời là đạt yêu cầu nhận thức cuối cùng.
4. Kiến trúc 13 module và phụ thuộc
4.1. Danh mục và phiên bản dự kiến
Tầng
Module
Phiên bản khởi điểm dự kiến
Trách nhiệm
Rust
match_contracts
4.0.0
Kiểu dữ liệu chung, đơn vị, DTO, trait ports, hợp đồng, version
Rust
match_physics
4.0.0
Động học, bóng, tiếp xúc, thực thi kỹ thuật
Rust
match_rules
4.0.0
Luật, trọng tài, lợi thế, restart, thể thức, xác thực luật thay người
Rust
player_condition
4.0.0
Thể lực, chấn thương, trạng thái tâm lý
Rust
match_tactics
4.0.0
Tổ chức tập thể, vai trò, chỉ đạo, lan truyền, bố trí bóng chết
Rust
player_intelligence
4.0.28
Nhận thức và ý định cá nhân
Rust
coach_intelligence
4.0.0
Kế hoạch trước trận và MatchCommand của HLV AI
Rust
match_engine
4.1.1
World State, DES, commit, vòng đời, checkpoint, áp dụng lệnh
Rust
match_telemetry
4.0.0
Stats, ratings, xG, diagnostics
Rust
football_match_engine_jni
4.0.0
ABI/JNI, handle, serialization, một .so(libfootball_match_engine.so)
Android
match_playback
4.0.0
Timeline, UI, lệnh, checkpoint trình diễn
Android
match_commentary
4.0.0
Lời bình, cue, TTS/WAV
Android
match_audio
4.0.0
Hiệu ứng, khán đài, mix và sound pack
Phiên bản ở bảng là định danh kế hoạch cho các crate mới. Giữ định danh hành vi cũ nếu exact parity, kể cả khi đổi phiên bản đóng gói. Không đổi đồng loạt version chỉ vì tách tệp.
4.2. Đồ thị phụ thuộc mã nguồn (A → B: A import B)
flowchart TD
    J[football_match_engine_jni] --> E[match_engine]
    J --> K[match_contracts]
    E --> P[match_physics]
    E --> R[match_rules]
    E --> C[player_condition]
    E --> T[match_tactics]
    E --> PI[player_intelligence]
    E --> CI[coach_intelligence]
    E --> M[match_telemetry]
    E --> K
    P --> K
    R --> K
    C --> K
    T --> K
    PI --> K
    CI --> K
    M --> K

match_contracts không phụ thuộc crate nghiệp vụ khác. Các crate lá chỉ phụ thuộc match_contracts trong kiến trúc đích; được dùng thư viện bên thứ ba đã kiểm soát. Engine gọi chúng qua giao diện có kiểu, không cho chúng gọi ngược Engine. JNI phụ thuộc Engine và Contracts, không gọi trực tiếp AI/Physics. Integration test harness dùng nhiều crate phải đặt ở crate/target test ngoài production DAG.
Xử lý điểm nghẽn phụ thuộc của player_intelligence (Đã kiểm toán):
Trong mã nguồn 
c2c392d8, player_intelligence/ đang import trực tiếp DeterministicRng (4 tệp), v4::ball_play (build_pass_trajectory, pass_reach_candidate, offside_reference_progress_m), v4::movement (evaluate_local_context, evaluate_passing_lane, player_attrs), v4::interaction (GroundContactGeometry, missed_ball_body_overlap, BallTrajectory, PassReachCandidate), v4::rules (estimate_reaction_decision_ms, nominal_ball_flight_ms, BallFlightKind, TimingModelConfig), v4::state::MatchStateV4 và v4::input::MatchInputV4.
Để giữ đúng DAG lá → match_contracts mà CẤM biến match_contracts thành monolith (Rủi ro R3) và vẫn giữ exact parity (HC-02) trong Phần A, BẮT BUỘC áp dụng một trong hai cơ chế (chốt tại ADR-004):
1. Phương án ưu tiên (Trait Port hẹp trong match_contracts): match_contracts chỉ định nghĩa kiểu dữ liệu và 2 trait cổng hẹp SpatialForecastPort (truy vấn dự báo không gian/tiếp xúc/timing từ dữ liệu quan sát) và RngSampleSource (lấy mẫu ngẫu nhiên giữ đúng thứ tự gọi legacy). match_engine (ủy quyền lõi toán học cho match_physics/match_rules) implement hai trait này và truyền vào player_intelligence. Toàn bộ công thức vật lý/luật vẫn nằm ở match_physics/match_rules, không bị copy vào match_contracts hay player_intelligence.
2. Phương án dự phòng có thời hạn (Ngoại lệ DAG Phần A): Chỉ dùng nếu trait port làm phát sinh chi phí chuyển đổi quá lớn trong A5; cho phép một adapter mỏng tạm thời trong match_engine bọc gọi chéo, ghi ADR và bắt buộc đóng cổng khi hoàn tất B2.
BẮT BUỘC kiểm tra DAG bằng cargo metadata --format-version 1 và allowlist dependency trên CI (bao gồm feature production, build-dependencies và phụ thuộc bắc cầu nội bộ). Không re-export MatchState hoặc API RNG toàn cục từ match_contracts.
4.3. Cấu trúc repository đích
rust/
  rust-toolchain.toml         # ghim phiên bản Rust cố định cho local và CI
  Cargo.toml                  # workspace, resolver, profiles (release panic = "unwind")
  Cargo.lock                  # khóa bắt buộc cho build tái lập
  crates/
    match_contracts/
    match_physics/
    match_rules/
    player_condition/
    match_tactics/
    player_intelligence/
    coach_intelligence/
    match_engine/
    match_telemetry/
    football_match_engine_jni/  # [lib] name = "football_match_engine", crate-type = ["cdylib"]
  tests/                      # integration harness & 54-attribute wiring checks ngoài crate lá
app/                          # ba ranh giới Android (package boundary trước, Gradle module sau)
tools/                        # build, kiểm tra DAG, parity, kiểm chuẩn
fixtures/                     # input/seed/expected trace có manifest
docs/                         # FOOTBALL_SIMULATION_4_SPEC.md, ADR, evidence, archive

Lưu ý đặt tên thư viện động: crates/football_match_engine_jni/Cargo.toml BẮT BUỘC giữ [lib] name = "football_match_engine" để xuất ra đúng libfootball_match_engine.so, tương thích trực tiếp với NativeBridge.kt, tools/build_lab_rust_android.sh và build-apk.yml.
5. Hợp đồng chi tiết các crate Rust
5.1. match_contracts: ngôn ngữ chung
Sở hữu kiểu MatchInput, MatchResult, PlayerId, TeamId, EventId, CommandId, SimTimeMs, DurationMs, Vec2/Vec3, PlayerAttributes54, MatchEnvironment, RulesetConfig (schema), TimingModel (schema), Observation, Intent, MatchCommand, MatchEvent, TelemetryEvent, VersionManifest4, mã lỗi, trait cổng (SpatialForecastPort, RngSampleSource), interface kết quả và quy cách fingerprint. Sở hữu toán hình học thuần nền tảng khi không chứa chính sách vật lý/bóng đá. Kiểm tra miền dữ liệu và cấu trúc; validation phụ thuộc thể thức thuộc Rules, validation trạng thái hiện thời thuộc Engine.
Không sở hữu World State runtime, công thức tính xác suất, chiến thuật, cơ chế thể lực, thuật toán chấm điểm, JNI, I/O thiết bị, hay RNG master. Dùng serde kiểm tra trường bắt buộc, enum không nhận diện, dữ liệu không hữu hạn và giới hạn kích thước.
Nguồn di chuyển: v4/spatial.rs, v4/attributes.rs, v4/input.rs, v4/events.rs, v4/error.rs, các phần hợp đồng của clock.rs, fingerprints.rs, rng.rs. Tách toán nền khỏi công thức nghiệp vụ; không chuyển nguyên trạng tất cả tệp vào contracts.
5.2. match_physics: thực tại cơ học
Đầu vào: snapshot vật lý tối thiểu (vị trí/vận tốc/hướng, hình thể và năng lực hiệu dụng, trạng thái bóng, điều kiện môi trường), intent đã xác thực, khoảng thời gian hoặc mốc dự đoán, nguồn ngẫu nhiên thực thi được cấp. Đầu ra: MotionPlan, PhysicalDelta, ContactFacts, BallTrajectory, BoundaryCrossing, dữ kiện quãng đường/cường độ và đề xuất mốc tương tác tiếp theo. Physics không tự commit World State, không tự quyết định tính hợp lệ hay tăng tỷ số.
Phải mô tả vị trí và vận tốc liên tục giữa sự kiện; giới hạn tốc độ, gia tốc, giảm tốc, góc đổi hướng, thời gian phản ứng theo năng lực; bóng lăn, bay, nảy, lực cản và biến thiên mặt sân/thời tiết. Kỹ thuật sút, chuyền, đón bóng, rê, phá, đánh đầu, bắt/đẩy/đấm bóng là cơ chế thực thi, không phải quyết định có nên làm. Một cú chuyền sai tạo quỹ đạo sai và cơ hội tiếp cận thật; không trao bóng tự động cho cầu thủ gần nhất. Chạm bóng không đồng nghĩa kiểm soát. Sau tiếp xúc phải xét chuyển động tiếp theo và tranh chấp bóng hai.
Phân biệt dự đoán tiếp xúc với tiếp xúc xác nhận: khi điều kiện thay đổi, hủy hoặc tính lại dự báo hợp lệ tại mốc sự kiện, không để kết quả dự báo cũ teleport cầu thủ. Local adaptive substep được phép để tìm thời điểm giao cắt/tiếp xúc; không biến DES thành vòng lặp tick gameplay toàn sân hoặc làm thay đổi RNG theo bước tìm kiếm.
Bài test bắt buộc: chuyển động thẳng và đổi hướng, giới hạn tốc độ, stop/restart, mirror sân, dt split trong điều kiện cố định, quỹ đạo biên, tiếp xúc trước/sau biên, bóng bổng ngoài tầm, sai kỹ thuật nhưng được cứu, khống chế lỗi, nhiều người tiếp cận, va chạm năng lượng tương đối, không dịch chuyển tức thời ngoài restart.
Nguồn di chuyển: v4/movement.rs, v4/interaction.rs, v4/ball_play.rs, v4/shooting.rs, v4/goalkeeper.rs, phần thực thi kỹ thuật của v4/set_pieces.rs (sau khi tách nhận thức, luật, state delta và thống kê).
5.3. match_rules: luật và trọng tài
Đầu vào: RuleContext chứa thể thức, trạng thái pháp lý, facts vật lý, người chơi bóng cuối cùng, snapshot việt vị, trạng thái trọng tài, thời gian, thẻ và lệnh hợp lệ cần xét. Đầu ra: RuleDecision, RestartSpec, AdvantageDecision, DisciplineDelta, điều kiện kết thúc hiệp/trận, kết quả xác thực cửa sổ thay người, lý do quyết định. Rules sở hữu chính sách; Engine lưu trạng thái pháp lý authoritative và lên lịch/commit.
Các luật phải được phiên bản hóa theo bộ luật/thể thức đã chọn. Luật 11 phải lưu dữ kiện thời điểm bóng được chơi, phân biệt vị trí việt vị và tham gia tình huống, bóng bật/chạm vô tình và chơi bóng có chủ ý theo ruleset. Không đổi dữ kiện vì cầu thủ chạy đến vị trí mới sau khi bóng rời chân. Physics cung cấp facts, Rules xử lý bóng vượt toàn bộ biên, loại restart, bàn thắng, phản lưới, phạm lỗi, thẻ, lợi thế, penalty và các trường hợp không hợp lệ. Tình huống đồng thời phải có trình tự ưu tiên minh bạch: không tự tăng bàn trước khi quyết định luật có hiệu lực.
Test: ném biên/phát bóng lên/phạt góc theo người chạm cuối; bàn hợp lệ/không hợp lệ; phản lưới; offside hai hướng sân; lợi thế thành công/thất bại; phạm lỗi trong/ngoài khu vực; thẻ vàng thứ hai, thẻ đỏ, cầu thủ rời sân; thay người đúng cửa sổ (kể cả thay nhiều người trong một lượt dừng và thay giữa hiệp); hết hiệp có bóng chết/penalty; hiệp phụ/luân lưu theo thể thức.
Nguồn di chuyển: v4/rules.rs, v4/own_goal.rs, phần xác thực cửa sổ/quyền thay người trong v4/ai_manager.rs (apply_manual_substitution_group), phần pháp lý trong v4/session.rs, v4/ball_play.rs, v4/shooting.rs, v4/set_pieces.rs.
5.4. player_condition: trạng thái sinh lý và tâm lý
Đầu vào: thuộc tính nền, condition trước đó, ActivitySample (khoảng ms, vận động thực, tăng/giảm tốc, cường độ, tải va chạm, nghỉ), ngữ cảnh sự kiện bóng đá đã commit, nguồn ngẫu nhiên injury/form được cấp. Đầu ra: ConditionDelta, trạng thái hiệu dụng, nguy cơ/chẩn đoán chấn thương theo mô hình, giới hạn vận động và thông tin tâm lý. Không tự dừng trận hoặc sửa thuộc tính nền; Rules/Engine xử lý gián đoạn hợp lệ.
Hai bể aerobic/anaerobic có miền xác định, cơ chế tiêu hao và phục hồi theo tải. Không tính cùng khoảng thời gian hai lần trong duel và world advancement. Phục hồi giữa hiệp là chuyển trạng thái riêng, không bắt buộc thời gian chờ trình diễn 15 phút. Phân biệt Base Attributes (không đổi trong trận), Match Condition (động), Effective Capabilities (suy ra dùng để thực thi). Tự tin và áp lực tâm lý cập nhật theo sự kiện có ý nghĩa; tránh vòng lặp «đội đang thắng càng mạnh vô hạn».
Test: cùng tải và khoảng thời gian với cách chia dt khác nhau; nghỉ không tiêu hao sprint; mệt mỏi theo cường độ; không âm/vượt miền; điều kiện chấn thương theo exposure; trạng thái reset/restore; tách khả năng hiệu dụng và nền; context big match và phong độ.
Nguồn di chuyển: v4/psychology.rs, phần TwoPoolFatigueState/InjuryKind/PlayerPsychologyState trong v4/state.rs, logic liên quan trong v4/session.rs.
5.5. match_tactics: tổ chức và phối hợp tập thể
Đầu vào: đội hình hợp lệ, team instructions, player roles/duties, tactical phase, ngữ cảnh tổ chức được phép, lệnh đã xác thực, thời điểm áp dụng. Đầu ra: TacticalAssignment, TeamShape, nhiệm vụ pressing/cover/screen/mark, kế hoạch chạy chỗ, set-piece layout, kế hoạch lan truyền chỉ đạo và mục tiêu vị trí. Engine quản lý thời điểm hiệu lực; Physics thực thi di chuyển; PI quyết định phản ứng cá nhân.
Bốn pha đích: In Possession, Out of Possession, Attacking Transition, Defensive Transition. Giữ ánh xạ tương thích trong Phần A; nâng cấp pha thực sự ở B3. Pressing phải phân công người áp sát, người khóa hướng, người bọc lót; không cho mọi cầu thủ cùng đuổi một bóng. Cùng cấu hình chiến thuật phải có hiệu lực thống nhất cho HLV AI và người chơi. Set-piece layout thuộc Tactics, thực hiện cú đá/di chuyển thuộc Physics, tính hợp lệ thuộc Rules.
Test: đội hình hợp lệ; mirror; phân công không trùng vô lý; hỗ trợ góc chuyền; các khối pressing có hành vi khác nhau; lệnh lan truyền đúng thời điểm; đổi vai trò sau thẻ đỏ/thay người; cùng lệnh AI/người chơi có cùng hiệu lực.
Nguồn di chuyển: v4/tactics.rs, phần dàn xếp đội hình trong v4/set_pieces.rs, phần phối hợp tập thể hiện nằm rải ở player_intelligence/support.rs và defensive.rs, cùng phần định nghĩa sơ đồ/style/slot mặc định trong LabTacticalPresets.kt (không chuyển logic UI, bản dịch hay thuật toán chọn XI tự động của HLV vào Tactics).
5.6. player_intelligence: nhận thức và quyết định cá nhân
PI là bộ não cầu thủ, không phải bộ giải kết quả. Đầu vào: PlayerObservation, nhiệm vụ chiến thuật, trạng thái trí nhớ PI của chính cầu thủ, cổng SpatialForecastPort và RngSampleSource được cấp. Đầu ra: DecisionPlan/ActionIntent, thời gian quan sát/quyết định, mục tiêu và điều kiện tái đánh giá, delta trí nhớ, diagnostics lựa chọn. Không nhận MatchStateV4, MatchInputV4, lịch sự kiện thật hoặc thuộc tính ẩn đối phương trong kiến trúc đích.
Phân biệt P_success chủ quan với kết quả thật. Trong B2 phải kiểm tra lại utility tắc bóng của 4.0.28 trên Physics mới, không mặc định giữ nguyên mọi trọng số. Thời gian suy nghĩ và thời gian chuẩn bị được khai báo rõ để Engine lên lịch, không làm trùng thời gian vận động đã tích phân.
Nguồn hiện có: 13 tệp trong src/player_intelligence/ (mod, decision, intent, utility, timing, on_ball, off_ball, defensive, support, goalkeeper, pass_forecast, tackle, version). Tệp tương thích v4/perception.rs được gỡ sau khi các call site trong v4/checkpoint.rs và test chuyển sang player_intelligence (hoặc giữ re-export tạm trong A3–A5 nếu parity test cần). Test cô lập quyết định, thông tin ẩn, hai thế giới có Observation giống nhau nhưng khác thuộc tính ẩn phải trả cùng quyết định khi cùng mẫu ngẫu nhiên và trí nhớ.
5.7. coach_intelligence: trí tuệ HLV AI
Trước trận: nhận CoachObservation chứa đội hình, khả năng được phép biết, thể lực/phong độ, thông tin trinh sát, thể thức, kế hoạch đã lưu; trả PreMatchPlan gồm đội hình, sơ đồ, role/duty, phân công bóng chết và chỉ đạo. Kế hoạch phải qua validation giống kế hoạch người chơi trước khi tạo trận.
Trong trận: nhận quan sát hợp lệ tại mốc ra quyết định (tỷ số, thời gian, thẻ, chấn thương, thể lực đội mình, khu vực bị khai thác, thống kê/quan sát được phép); trả MatchCommand có command_id, thời điểm yêu cầu, mục tiêu, nội dung và nguồn AI. Không sửa đội hình hoặc MatchState trực tiếp. Chịu chung số quyền thay người, cửa sổ áp dụng, điều kiện bóng chết và độ trễ lan truyền.
Test: đội thiếu người, chấn thương, cầu thủ mệt, đang dẫn/đang thua, không còn quyền thay, lệnh trùng/không hợp lệ, equivalence AI vs người chơi, dữ liệu đối thủ bị che.
Nguồn di chuyển: phần quyết định AI (evaluate_ai_managers) trong v4/ai_manager.rs (tách khỏi apply_manual_substitution_group vốn thuộc Engine/Rules); phần thuật toán chọn đội hình tối ưu và phân công bóng chết trước trận (planOptimalLineup, buildAutoSetPiecePlan) trong LabTacticalPresets.kt được quy hoạch chuyển hóa thành PreMatchPlan ở B4, không trộn vào Phần A.
5.8. match_engine: điều phối và commit
Sở hữu MatchState, runtime ball/player state, Scheduler, clock, trạng thái phase/period, trạng thái luật và chiến thuật đang hiệu lực, condition và memory runtime, RNG master/các luồng, command ledger, pending intent/motion, checkpoint, sequence và điểm commit. Thực hiện implement SpatialForecastPort và RngSampleSource khi gọi PI. Không chứa công thức xác suất chuyền/sút/tắc, tính lợi ích AI, gia tốc, tiêu hao thể lực, xử lý thẻ cụ thể hoặc chấm điểm.
Engine gọi crate bằng snapshot/DTO tối thiểu, nhận kết quả, xác thực delta, resolve xung đột theo thứ tự chuẩn, commit nguyên tử và phát sự kiện. Không cho crate lá giữ &mut MatchState. Thực hiện FullEvent và FastBatch trên cùng transition kernel. Rút gọn session.rs (hiện 4.221 dòng) theo quyền sở hữu thành các module có chức năng thực (session, clock, scheduler, world, transition, commands, period, restart, observation_builder, checkpoint, replay, event_commit), không đặt mốc dòng cứng.
Nguồn di chuyển: v4/session.rs, v4/scheduler.rs, v4/checkpoint.rs, v4/state.rs, phần áp dụng thay người của v4/ai_manager.rs (apply_manual_substitution_group), phần runtime clock.rs và rng.rs ở root.
5.9. match_telemetry: sự thật đo lường
Nhận MatchEvent, TelemetryEvent, trạng thái tổng hợp tối thiểu/initial context có kiểu do Engine cung cấp. Sở hữu accumulator thống kê, chấm điểm, mô hình xG độc lập có định danh, diagnostics và serialization. Không đọc/mutate Engine, không có RNG gameplay, không phản hồi quota.
Nguồn di chuyển: v4/stats.rs, v4/diagnostics.rs (1.323 dòng), ratings hợp lệ hiện có. Các công cụ Monte Carlo/audit/fixture (v4/calibration.rs, v4/attribute_impact_tests.rs, v4/pi_parity_fixture.rs, và bộ test tích hợp trong v4/tests.rs) chuyển sang integration test harness rust/tests/ hoặc tools/, không gắn vòng lặp kiểm chuẩn vào runtime production.
5.10. football_match_engine_jni: biên giới ứng dụng
Một cdylib duy nhất cho Android với [lib] name = "football_match_engine" (xuất libfootball_match_engine.so), không chứa logic bóng đá. Quản lý handle theo phiên, bảo vệ đồng thời, kiểm tra input/size/schema, mã lỗi typed, close idempotent; không dùng handle đã đóng hoặc lẫn match id. Giữ panic = "unwind" trong [profile.release], bắt unwind ở biên FFI; không cho panic thoát sang JVM.
Kiểm kê JNI chính xác tại c2c392d8:
• 9 hàm V3 legacy cần gỡ ở A1 (sau khi gỡ mọi call site Kotlin): 6 hàm unsafe raw-vtable trong jni_bridge.rs (nativeSimulateMatchJson, nativeCreateSession, nativeSubmitCommand, nativeAdvanceSession, nativeFinishSession, nativeDestroySession) và 3 hàm trong lib.rs (nativePortVersion, rngSample, nativeCoreSelfCheck).
• 15 hàm V4/DES giữ lại và chuyển vào football_match_engine_jni: 2 hàm phiên bản hiện nằm ở lib.rs (nativeBridgeVersion, nativeEngineProtocolVersion) và 13 hàm trong jni_bridge.rs (nativeSimulateMatchV4Json, nativeSimulateBatchV4Json, nativeCreateV4Session, nativeSubmitV4Command, nativeSubmitV4CommandJson, nativeAdvanceV4Session, nativeAdvanceV4SessionUntilMs, nativeStartNextPeriod, nativeExportV4Checkpoint, nativeImportV4Checkpoint, nativeFinishV4Session, nativeDestroyV4Session, nativeReplayV4Json), bổ sung API trả VersionManifest4/capabilities.
6. Vòng đời DES và quyền sở hữu trạng thái
6.1. Ba loại sự kiện và một đường commit
Phân biệt ScheduledEvent (dự kiến trong tương lai, nội bộ), PhysicalFacts (dữ kiện cơ học tại mốc, chưa kết luận luật), MatchEvent (sự kiện gameplay đã commit). Không phát dự báo như thể đã xảy ra. Một intent có thể bị hủy trước khi thực hiện; không tính chuyền, cú sút hoặc phạm lỗi nếu hành động chưa phát động/xác nhận theo định nghĩa thống kê.
flowchart TD
    A[Observation] --> B[PI or Coach Intent]
    B --> C[Engine Validate and Schedule]
    C --> D[Advance Physical World to Event Time]
    D --> E[Physics Contact Facts]
    E --> F[Rules Decision]
    F --> G[Condition and Tactical Deltas]
    G --> H[Validate and Commit Atomic Transition]
    H --> I[Schedule Next Events]
    H --> J[MatchEvent]
    H --> K[TelemetryEvent]
    J --> L[Android Presentation]

Trình tự cho một sự kiện tại t: lấy sự kiện hợp lệ tiếp theo; tích phân từ mốc hiện tại tới t theo điều kiện đang hiệu lực, không vượt mốc tương tác sớm hơn; tính facts; phân xử luật; tính delta điều kiện/chiến thuật; kiểm tra invariant; commit trạng thái, RNG, scheduler, ledger và event sequence như một transition nguyên tử; phát event/telemetry đã commit; lên lịch các hành động kế tiếp. Nếu lỗi trước commit, không để score, RNG, counters hoặc event log ở trạng thái nửa chừng.
6.2. Đồng hồ và ưu tiên Scheduler
• sim_time_ms nguyên i64, tăng đơn điệu; period_elapsed_ms và thời gian hiển thị được suy ra có hợp đồng. Không dùng wall clock hoặc tốc độ thiết bị làm input gameplay.
• Các hiệp chính 45 phút (2.700.000 ms), hiệp phụ theo Ruleset (900.000 ms/hiệp), bù giờ công bố và xác nhận theo luật. Đồng hồ vẫn tiến trong bóng chết hợp lệ. HalfTimeBreak là trạng thái riêng, không tích phân thời gian thi đấu trong lúc nghỉ giữa hiệp.
• Scheduler dùng khóa sắp xếp tổng (time_ms, priority, sequence_id). Giữ nguyên thứ tự ưu tiên hiện tại trong Phần A; mọi thay đổi ưu tiên là thay đổi hành vi (loại B). Không dùng thứ tự duyệt HashMap hoặc thread scheduling làm tie-break.
• Mỗi dự báo có scope và generation/token hủy. Hủy đúng nhóm khi có bóng chết, cầu thủ rời sân, chuyển hiệp hoặc lệnh đổi điều kiện; không vô tình hủy mốc hết hiệp.
• Xử lý nhiều sự kiện cùng timestamp không lặp vô hạn; giới hạn watchdog báo lỗi và seed tái lập, không âm thầm bỏ sự kiện. advance_until công bố rõ trả tại mốc sự kiện cuối hay horizon; polling giữa hai sự kiện không tạo transition nghiệp vụ.
6.3. Vòng đời trận đấu và bóng chết
Trạng thái tối thiểu: Created → PreMatch → FirstHalf → HalfTimeBreak → SecondHalf → FullTime hoặc chuyển tiếp ExtraTimeFirstHalf → ExtraTimeBreak → ExtraTimeSecondHalf → Shootout → Finished khi Ruleset yêu cầu. Trong một period, biểu diễn nhất quán LivePlay, DeadBall và trạng thái chờ lợi thế. Không coi luân lưu là tiếp tục đồng hồ thi đấu chính; MatchResult chỉ đóng một lần.
Quyền sở hữu bóng chết
Module
Dừng hay tiếp tục; lợi thế; đội hưởng và loại restart
Rules
Vị trí thực tế vượt biên/tiếp xúc, vận động chuẩn bị và thực hiện
Physics
Người thực hiện, bài bản, vị trí mục tiêu
Tactics, PI
Điều kiện hợp lệ để bóng vào cuộc
Rules
Mốc chuẩn bị, xác nhận, hủy, thực hiện và tiếp tục
Engine
Tiêu hao/hồi phục trong giai đoạn
Condition
Sự kiện, số lần và thời lượng đã xảy ra
Telemetry
Mỗi vòng đời restart có restart_id, lý do, đội, vị trí, mốc bắt đầu, mốc bóng trở lại cuộc chơi hoặc hủy. Cả ném biên, phạt góc, phát bóng lên, đá phạt, penalty, kickoff và drop ball (nếu hỗ trợ) đi qua một đường commit duy nhất.
6.4. Lệnh và tính nhất quán
MatchCommand gồm ID idempotent, match ID, nguồn, payload, mốc yêu cầu, context/version; Engine ghi mốc nhận, xác thực, chấp nhận/từ chối, kế hoạch hiệu lực và mốc thực sự hiệu lực. Phân biệt Accepted, Pending, Applied, Rejected, Cancelled. Lệnh gửi lại cùng ID không thực thi hai lần; cùng ID khác nội dung là lỗi. Kiểm tra quyền cầu thủ/đội, số lượt/cửa sổ thay người, trạng thái trận, cầu thủ bị đuổi/chấn thương. Tái xác thực trước khi áp dụng nếu điều kiện thay đổi. Coach AI đi qua cùng queue và validation với người chơi.
6.5. Invariant bắt buộc (runtime và kiểm thử)
ID
Invariant
INV-01
Bóng kiểm soát có đúng một chủ hợp lệ, đúng đội; bóng tự do/in-flight không có chủ kiểm soát. Last touch là trường riêng.
INV-02
Tọa độ, tốc độ, gia tốc, quỹ đạo hữu hạn; không teleport hoặc vượt ngân sách vận động ngoài tái bố trí được luật quy định. Vượt biên được xử lý, không clamp mất bằng chứng.
INV-03
Clock không lùi, không thực thi trước thời điểm, mỗi sequence/sự kiện authoritative commit một lần.
INV-04
Cầu thủ bị đuổi/rời sân không tiếp tục tranh bóng; đội hình và thay người thỏa Ruleset.
INV-05
Bàn hợp lệ được ghi đúng một lần, phản lưới đúng đội hưởng, bàn luân lưu riêng.
INV-06
Khi bóng chết, không có hành động live play trái phép; các event hết hiệp không bị mất do hủy live play.
INV-07
Từng khoảng tích phân vận động/thể lực được ghi một lần; condition trong miền hợp lệ; không sửa base attributes.
INV-08
Mọi delta tham chiếu ID tồn tại và trạng thái được phép; transition nguyên tử, lỗi không để nửa kết quả.
INV-09
Checkpoint khôi phục đủ state có ảnh hưởng tương lai, kể cả RNG, queue, pending intent và ledger.
INV-10
Telemetry không thay gameplay; tổng thời gian bóng sống/chết khớp đồng hồ thi đấu theo quy ước; không ghi trùng hành động.
INV-11
MatchEvent có match ID, event ID/sequence duy nhất và mốc thời gian; UI không áp dụng event của phiên khác.
INV-12
Kết thúc trận đóng Result một lần; không còn lệnh/action live hợp lệ sau khi hoàn tất.
Không được sửa invariant thất bại bằng bỏ qua test, nới epsilon tùy ý hoặc buộc state về giá trị đẹp.
7. Dữ liệu, nhận thức, RNG và tính tất định
7.1. Đơn vị và hợp đồng số học
Sử dụng mét, m/s, m/s², radian, mili-giây i64, đơn vị thể lực/thuộc tính có miền rõ ràng. Một hệ tọa độ sân vật lý duy nhất (105 × 68 m giữ nguyên trong Phần A); kích thước, khung thành, biên và vùng luật nằm trong hợp đồng chung. Không trộn chỉ số 1–20 với giá trị 0–1 hoặc 0–100 mà không chuyển đổi rõ.
Tách hai mức determinism:
1. Exact determinism trong môi trường đã khóa: cùng input/config/seed/command log trên cùng toolchain/target đã khóa (rust-toolchain.toml + Cargo.lock), hành vi và state ảnh hưởng tương lai phải khớp bit ở các trường chuẩn hóa khi FullEvent/FastBatch, polling, checkpoint, telemetry on/off và refactor thuần túy.
2. Cross-platform determinism (Linux x86_64 vs Android ARM64): kiểm chứng riêng toolchain, FMA, hàm lượng giác/căn/mũ, serialization float (serde_json với float_roundtrip), thứ tự giảm tổng, tie-break. Chưa có bằng chứng thì giữ trạng thái Chờ xác minh; nâng cấp deterministic math nếu cần là thay đổi hành vi riêng (loại B).
dt invariance: khi không có sự kiện đổi điều kiện, đồng hồ, counter, event ordering phải exact; đại lượng liên tục có dung sai số học đã ghi và không làm đổi nhánh gameplay. Không rút RNG mới vì polling hoặc substep tìm giao cắt.
7.2. Ba lớp dữ liệu
Dữ liệu
Người nhận
Nội dung
PhysicalWorldSnapshot
Physics/Engine
Thực tại cơ học tại mốc xác định, năng lực hiệu dụng và môi trường cần thiết.
PlayerObservation
PI
Self, bóng/người quan sát được, nhiệm vụ, ngữ cảnh, tuổi quan sát và độ tin cậy, dự báo chủ quan quaSpatialForecastPort.
CoachObservation
Coach
Thông tin đội mình, scouting được phép, diễn biến đã xảy ra, chỉ số được UI/HLV cung cấp.
Engine sở hữu observation builder và chính sách cấp dữ liệu; không đưa actual_future_contact, kế hoạch bí mật đối phương, random outcome, hidden attributes hay event queue riêng vào DTO đích. Chuyển đổi bắt buộc hai bước: (A) CompatibilityObservation + SpatialForecastPort giữ exact parity, ghi inventory ngoại lệ trường và ADR; (B) thay bằng observation hữu hạn ở B2/B4, tăng behavior ID, đóng ngoại lệ.
7.3. RNG và quyền truy cập
Đích: Engine sở hữu nguồn ngẫu nhiên, phát stream/capability riêng cho Physics, Rules, Condition, PI, Coach; Android Commentary và Audio dùng nguồn độc lập. PI/Coach chỉ gọi RngSampleSource, không đọc/sửa seed hay RNG giải kết quả. Trong Phần A, giữ nguyên thuật toán DeterministicRng và thứ tự draw legacy để bảo toàn exact parity; tách stream thực sự ở commit hành vi loại B có phiên bản riêng. Không thêm draw trong debug, log hay Telemetry.
8. Phiên bản, checkpoint và replay
8.1. VersionManifest4 và compatibility
Một manifest duy nhất dùng kiểu sở hữu (String/enum) có serde:
• BuildIdentity: commit, toolchain, target, profile, Cargo.lock hash, phiên bản đóng gói từng crate.
• DataSchemas: protocol (hiện v5), event schema (hiện v5), checkpoint schema (hiện v2), input/result schema, telemetry schema, canonical fingerprint version; JNI bridge version (hiện v8) nằm riêng.
• GameplayIdentity: behavior ID của từng crate nghiệp vụ (match_engine = 4.1.1, player_intelligence = 4.0.28 pi4.0-second-ball-state-value, ...), RNG algorithm/version, numeric policy, event ordering và timing policy.
• MatchConfigIdentity: input chuẩn hóa (đủ để tái lập replay), input hash, seed, ruleset hash/version, timing model hash, dataset/roster hash.
• PresentationIdentity: playback/commentary/audio version, ngôn ngữ, mode, sound pack, cue schema; không vô hiệu hóa Rust checkpoint.
SimulationCompatibilityId tính bằng canonical serialization + hash các phần ảnh hưởng gameplay/schema continuation. Không đưa build path, timestamp hoặc Android app version vào compatibility lõi. Refactor có exact parity giữ nguyên GameplayIdentity; thay đổi hành vi tăng phiên bản đúng crate tương ứng (4.x.y). Mặc định từ chối payload không tương thích kèm lý do rõ, không ghi đè dữ liệu cũ.
8.2. Bốn loại dữ liệu lưu
Loại
Phải lưu
Chính sách
Simulation checkpoint
Toàn bộ state tương lai, queue, intents, clock, RNG, commands, diagnostics accumulator, manifest
Chỉ restore khi tương thích hoặc migration đã kiểm chứng.
Seed replay
Input/config/seed, command log authoritative, behavior IDs, fingerprint mong đợi
Re-simulate bằng policy tương thích. Không âm thầm chạy policy mới.
Event replay
Initial presentation snapshot, event stream hoàn chỉnh cần thiết, schema và metadata
Đọc/phát mà không cần engine cũ nếu schema reader hỗ trợ. Không dùng làm checkpoint gameplay.
Presentation checkpoint
Event cursor, timeline, buffered committed events, cue ID/offset, mode/rate, manifest và match ID
Restore đúng phiên; mismatch xử lý riêng, không làm mất checkpoint Rust hợp lệ.
8.3. Ghi và khôi phục an toàn
Envelope có magic/type, schema, length, match ID, generation, compatibility ID, checksum, payload. Validate đầy đủ trước khi khôi phục. Ghi qua tệp tạm cùng filesystem (AtomicFile trên Android), flush/sync, atomic rename, giữ ít nhất một thế hệ hợp lệ trước đó. Dùng generation chung để ghép bộ Rust + presentation checkpoint. Chống double apply event/command bằng ledger/cursor. Kiểm tra truncate/corrupt, sai match ID, schema lạ, force-stop giữa ghi.
9. Android, lookahead, bình luận và âm thanh
9.1. Ba module trình diễn
• match_playback: quản lý session/JNI worker, event buffer, presentation timeline, score/clock UI từ frontier đúng, thao tác đội hình/chiến thuật, pause/rate, checkpoint/replay, lỗi và lifecycle. LabReferenceMatchActivity (hiện 1.356 dòng) chỉ bind UI, forward intent, render state. Nguồn: MatchPlaybackController.kt, LabV4PlaybackCoordinator.kt, LabRustLiveSession.kt, MatchCheckpointStore.kt, LabGlobalSeedReplayStore.kt, NativeBridge.kt. Mọi callback kiểm tra match ID và generation.
• match_commentary: chuyển MatchEvent đã commit thành cue text theo ngôn ngữ/mode/context, FIFO theo timeline, template evidence policy, narrative tracker, TTS/WAV, cue offset và khôi phục. Nguồn: LabCommentaryEngine.kt, LabCommentaryTemplatePolicy.kt, LabNarrativeTracker.kt, NarrationScheduler.kt, MatchTtsManager.kt. Lưu ý kỹ thuật: task validateCommentaryTokens trong app/build.gradle (chạy tại preBuild) đang đọc hard-code đường dẫn LabCommentaryEngine.kt và parse chuỗi 'val values = mapOf(' → '\n        )' cùng commentary_v2.xml, commentary_v3.xml; mọi thay đổi đường dẫn/định dạng ở A6 BẮT BUỘC cập nhật task Gradle này đồng thời. Không phát minh sự kiện/tỷ số nếu event không chứng minh.
• match_audio: hiệu ứng sân, nền khán đài, sound pack, catalog, audio focus, mix/ducking. Nguồn: LabAudioDirector.kt, MatchAudioManager.kt, SoundPackManager.kt, SoundCatalog.kt. Bộ 48 tệp OGG lõi trong app/src/main/assets/mod_audio/core/ cùng AUDIO_SHA256_MANIFEST.txt và AUDIO_LICENSES.md đã được xác minh khớp SHA-256 100% tại mốc c2c392d8. Không ảnh hưởng clock hoặc RNG lõi.
9.2. Frontier, lookahead và lệnh người chơi
Định nghĩa PresentationFrontier ≤ CommittedFrontier ≤ SimulationFrontier. Không lộ score, event hoặc lời bình của vùng speculative.
Thực trạng mã nguồn và lộ trình chuyển đổi (ADR-006):
• Hiện tại MatchPlaybackController.kt chưa triển khai rollback cho speculative lookahead, và tools/verify_v4_des_contract.py (CRITICAL-03) đang khóa hành vi không hủy cue đã xác nhận khi gửi lệnh chiến thuật có độ trễ.
• Khuyến nghị triển khai hai pha: Trong toàn bộ Phần A, giữ nguyên semantics hiện tại (hoặc thu nhỏ cửa sổ lookahead nếu cần) để bảo vệ exact parity và tránh làm phình rủi ro tái cấu trúc; chỉ triển khai speculative lookahead có rollback/re-simulation (khôi phục mốc committed gần nhất, chèn lệnh tại timestamp đang xem, mô phỏng lại vùng chưa trình diễn, thay buffer/cue theo generation mới, pause mặc định khi mở dialog chiến thuật) như một hạng mục loại I/B độc lập sau khi A7/A8 ổn định.
Test cho chính sách đích: gửi lệnh khi đang xem phút 35:00 trong lúc engine mô phỏng tới 35:40; lệnh không ảnh hưởng trước 35:00, không lộ tương lai, không mất/nhân đôi lệnh, đối chiếu với chạy không lookahead khi cùng authoritative command log.
9.3. Tốc độ phát và lời bình
Đồng hồ trình diễn neo vào SystemClock.elapsedRealtime() và presentation_sim_ms; đổi rate (1.0x, 1.25x, 1.5x, 2.0x, 3.0x) re-anchor tại đúng mốc hiện tại; pause freeze, resume không nhảy. Không để TTS/MediaPlayer điều khiển clock Rust.
Cùng event stream, ngôn ngữ, phiên bản Commentary, chế độ và seed biến thể phải cho cùng danh sách cue/text/ordering ở mọi tốc độ. Không auto mute ở 3.0x, không bỏ câu vì rate, giữ FIFO nghiêm ngặt. Tốc độ 4,2 âm tiết/giây là mốc hiệu chỉnh tiếng Việt khởi điểm (MatchTtsManager.kt), phải đo kiểm trên WAV/TTS thiết bị thật ở A8. Khi thiết bị không theo kịp, ghi backlog/lag và thông báo minh bạch theo lựa chọn được chốt tại ADR-007 (mặc định: giữ clock gameplay, phát FIFO không bỏ lời, báo rõ nếu còn bình luận sau tiếng còi).
9.4. Hợp đồng MatchEvent cho Commentary/Audio
Event envelope gồm: match ID, event ID/sequence, simTimeMs, period, kind, actor/target/side, vị trí, score/phase context đã xác nhận, metadata typed, schema. Bổ sung dần tín hiệu ngữ cảnh có dữ liệu thật ở Phần B (áp lực, khoảng cách/tốc độ sút, loại tiếp xúc, trúng khung gỗ, last touch, thẻ/lợi thế, lệnh HLV accepted/effective); không tạo fake metadata.
10. Telemetry, 54 thuộc tính và phương pháp kiểm chuẩn
10.1. Hai luồng dữ liệu, một định nghĩa thống kê
MatchEvent phục vụ gameplay/trình diễn; TelemetryEvent phục vụ đo lường nội bộ từ transition. Telemetry chỉ tổng hợp, không điều khiển gameplay. Nếu chỉ số chưa đo chính xác, ghi unavailable/approximate, không điền 0 giả.
Chỉ số
Định nghĩa cần khóa
Thời gian bóng sống/chết
Phân hoạch thời gian thi đấu thực tế theo luật và mốc clock; loại trừ nghỉ giữa hiệp và luân lưu khỏi mẫu 90 phút.
Đường chuyền
Lần bóng được chơi với ý định chuyền; success theo định nghĩa nhận bóng đã chọn. Không cộng lại khi bóng hai/offside.
Chuyền sai kỹ thuật
Phát bóng lệch ý định kỹ thuật (vẫn có thể được cứu); tách khỏi người nhận không tiếp cận được.
Mất quyền kiểm soát
Chuyển quyền kiểm soát thực tế hoặc restart theo định nghĩa khóa, không đồng nhất với pass failed.
Cắt bóng / tắc bóng
Tách candidate evaluation, committed challenge, actual contact, legal contact, controlled win, loose contact, foul, miss, advantage.
Bóng hai
Gắn origin event/cause ID, đội giành lại, thời gian, khoảng cách, boundary hoặc unresolved.
Cú sút
Hành động có ý định ghi bàn đã phát động; trúng đích/chặn/cột/xà/goal theo taxonomy cố định.
xG
Mô hình đánh giá cơ hội trước cú sút có version riêng; không dùng làm target ép tỷ số.
Phạm lỗi
Tách contact facts, phán quyết foul, advantage, stopped foul, thẻ, penalty với mẫu số rõ ràng.
Quãng đường
Tích phân chuyển động thực; không cộng tái bố trí bóng chết hoặc quãng đường giả.
Pressing
Số cơ hội, phân công, thời lượng áp sát, vùng thu hồi; không nhầm mẫu đánh giá với pha độc lập.
Thể lực
Trạng thái đầu/cuối và tải aerobic/anaerobic theo khoảng, cầu thủ và phút thi đấu.
Chấm điểm
Công thức minh bạch có version; không phản hồi ngược vào gameplay nếu chưa có hợp đồng Condition riêng.
Giữ quy tắc thống kê legacy trong Phần A để parity; sửa định nghĩa sai ở commit độc lập có cột đo song song. Báo cáo tổng hợp phải có mẫu số, đơn vị, cohort, số trận thành công/thất bại và missing values.
10.2. Mức ghi nhận và audit
Ba mức: Core (luôn chạy cho Result/checkpoint), Diagnostic (bộ đếm/histogram cơ chế), Trace (chi tiết tìm first divergence). Chuyển mức không đổi RNG, clock, event sequence hoặc MatchResult core. FullEvent và FastBatch dùng chung transition kernel. Đánh giá PI bằng ghép forecast–outcome theo ID tại đúng mốc; dùng Brier score/reliability bins cho xác suất thực.
10.3. Attribute Contract cho đủ 54 chỉ số
Kiểm toán v4/attributes.rs xác nhận schema PlayerAttributes54 gồm đủ 54 trường chia 5 nhóm: Technical (13), Mental (14), Physical (8), Goalkeeping (9), Hidden (10). Test hiện hành (v4/tests.rs) đã tách rõ 50 thuộc tính in-match và 4 thuộc tính career-only (professionalism, ambition, adaptability, loyalty). Dùng trực tiếp phân loại này làm mốc đầu vào để sinh attribute_contracts.json ở A-1.
Mỗi trường trong 54 thuộc tính phải có hồ sơ gồm: Attribute ID & group, Scope (in-match / pre-match / career-only / mixed), Owner (crate sở hữu chính sách và crate đọc năng lực hiệu dụng), Mechanism, Expected effect, Isolated fixture (mức 5/12/18 cố định biến khác), Population evidence (exposure, mẫu số, hiệu ứng ghép seed, khoảng bất định), và Acceptance status (Mapped / Isolated verified / Population observed / Validated / Career-only / Blocked). Không ép 4 chỉ số career-only tác động trực tiếp vào pha bóng để lấy thành tích 54/54.
10.4. Bộ fixture và ma trận Monte Carlo
Bảo tồn bộ seed chuẩn của workflow DES: baseline 1.000, balanced 128, high_press 64, low_block 64, strong_home 64, strong_away 64, attacking_sample 12 (tổng 1.396) cùng nhóm FullEvent 12 trận đối chiếu trace. Xác nhận manifest và checksum tại A-1.
Mở rộng dần ma trận cohort ở Phần B (cân bằng & mirror, đường cong sức mạnh nhiều mức, đội hình thực tế đa dạng theo vai trò, ma trận chiến thuật theo từng đội, luật đặc biệt/thiếu người/hiệp phụ/luân lưu, môi trường/mưa/fatigue, tình huống hiếm cố định, và chuỗi lệnh tương tác). Báo cáo đầy đủ mean, median, SD, P5/P95, min/max, phân bố tỷ số, paired delta theo seed và khoảng bất định; không loại outlier không giải thích.
10.5. Dải tham chiếu không phải quota
Dải tham khảo sơ bộ (bóng sống 55–62 phút, phạm lỗi 18–24/trận, chuyền 850–950, chuyền chính xác 79–84%, cú sút 23–27, bàn 2,65–2,95) chưa có nguồn giải đấu/mùa xác nhận; cần bổ sung dataset có nguồn rõ ràng theo ADR-010 trước khi dùng làm mốc realism. Mọi điều chỉnh tuân quy trình: phát hiện lệch → giả thuyết bác bỏ được → đo exposure → test cô lập → thay đổi nhỏ → đối chiếu quần thể → giữ/hoàn tác.
11. CI, ngân sách tài nguyên và tiêu chí nghiệm thu
11.1. Các loại thay đổi và cổng kiểm thử
Loại
Ví dụ
Cổng bắt buộc
R: Refactor thuần túy
Di chuyển crate/file, DTO/trait tương thích, xóa code chết
Cargo workspace --locked, DAG, static checks, exact parity bộ khóa & FullEvent, checkpoint continuation; APK nếu boundary/build liên quan.
B: Behavior
Công thức vật lý, luật, AI, timing, RNG, đổi thứ tự event
Cô lập mechanism, kiểm thử toàn bộ, 1.000+ cohort ghép seed, audit 54, bump behavior ID, compatibility, báo cáo tác động.
I: Integration/UX
Lookahead policy, command acceptance, restore, JNI, presentation
Test hành vi tích hợp, replay/command parity theo semantics mới, build APK và device tests bị ảnh hưởng. Nếu đổi gameplay, đồng thời loại B.
T: Test/evidence only
Sửa version literal cũ, thêm instrumentation, fixtures
Chứng minh không đổi behavior, kiểm tra test có ý nghĩa; không nới gate để giả xanh.
D: Docs/resource
ADR, tài liệu, license, sound pack
Kiểm tra link/manifest/license; audio UX test nếu resource đổi.
Không gộp R thuần túy với B. Cập nhật version test phải kiểm tra manifest/compatibility và negative case, không assert hai hằng cùng nguồn.
11.2. CI nhiều lớp và kế hoạch hợp nhất 10 workflow hiện hành
Repository hiện có 10 workflow chồng chéo (cả build-apk.yml và validate-des-repair.yml đều build ARM64 .so + APK trên push: main; validate-v4-android.yml gắn nhánh cũ; pi4-extraction-parity.yml ghim commit 964e56a3; tools/validate_v4_p1_full.sh tự skip khi thiếu cargo/kotlinc). Tại A0, lập coverage map và hợp nhất toàn bộ 10 workflow về 5 tầng cổng rõ ràng, đọc toolchain từ rust-toolchain.toml và chạy --locked:
1. L0 Static/Structure: format, lints, cargo metadata DAG allowlist, schema, license, checksum OGG/commentary, no-legacy production references. Fail cứng nếu thiếu tool (sửa validate_v4_p1_full.sh bỏ nhánh warning-skip).
2. L1 Unit & Mechanism: cargo test --workspace --all-targets --locked trên toolchain đã khóa; test crate riêng, property/mirror/units, unsafe FFI boundary. Ignored tests phải có lý do và job riêng nếu là gate.
3. L2 Integration & Parity: 1.396 case khóa theo manifest, 12 FullEvent trace, checkpoint exact continuation, command ledger, scheduler, replay, telemetry toggles, first divergence detector.
4. L3 Behavioral Evidence: population & attribute audit 904 (chỉ bắt buộc trước merge cho loại B; refactor dùng L2 parity).
5. L4 Android Integration & Build: Kotlin/JVM tests (run_des_playback_jvm.sh, run_commentary_regression.sh, run_v4_p1_jvm_smoke.sh), :app:validateCommentaryTokens, đóng gói libfootball_match_engine.so (ARM64) và APK Debug/Release — hợp nhất thành một luồng build duy nhất thay vì chạy trùng ở 3 workflow.
6. L5 Device Acceptance: nghiệm thu điện thoại thật tại A8 và khi tính năng trình diễn/hiệu năng đổi; nếu chưa có thiết bị, giữ trạng thái Chờ xác minh.
11.3. Exact parity protocol
A-1 tạo golden evidence cho từng trận: normalized input + seed, command log, event stream/trace, score, stats, state digest, RNG digest, scheduler digest, checkpoint markers, toolchain, target, fixture manifest. So sánh 5 cấp: Result; toàn bộ event gameplay; snapshot state; pending scheduler/intents/RNG/ledger; restore và tiếp tục. Chạy negative test cố ý đổi state nhỏ để xác nhận detector hoạt động. First divergence report ghi đủ seed, cohort, event sequence, sim time, expected/actual, field path, RNG digest, pending action và module xử lý. Không cập nhật golden để làm xanh test đỏ.
11.4. Benchmark và tài nguyên
Đo trên môi trường cố định (thiết bị/OS, CPU/ABI, RAM, pin, nhiệt, voice TTS, rate/mode, warmup, profile build). Đo Rust (matches/s FastBatch, event/s FullEvent, P50/P95/P99 latency, peak RAM, kích thước & thời gian save/restore checkpoint, JNI latency) và Android (render frame, thread stall, RAM, CPU, nhiệt, pin, lookahead underflow, backlog cue, drift P50/P95/max). Ngưỡng cụ thể khóa tại ADR-009 trước khi chạy A8. Đáp ứng tối thiểu: mượt ở 3.0x không ANR/crash/leak handle, không block UI, backpressure minh bạch khi quá tải.
11.5. Ma trận nghiệm thu thiết bị thật
Ca
Điều kiện PASS
Full 90 phút ở 1.0x, 2.0x, 3.0x
Cùng event/command log, score, nội dung cue theo mode; đo thời gian, drift, backlog, pin/nhiệt. Không auto mute.
Đổi rate giữa câu, pause/resume
Clock re-anchor đúng; không nhân đôi hoặc bỏ cue, không nhảy score/time.
Force-stop bóng sống, goal celebration, corner/free kick, halftime, pending command
Restore từ generation hợp lệ; không mất trạng thái checkpoint; continuation đúng, không đếm trùng.
Force-stop giữa TTS hoặc giữa ghi file
Khôi phục offset/cursor theo hợp đồng, không ghi đè checkpoint tốt.
Đổi app, khóa màn hình, mất audio focus, cuộc gọi
Tôn trọng chính sách lifecycle đã duyệt, không mất phiên/queue; kiểm tra hành vi AudioFocus theo targetSdkđã chốt.
Bộ nhớ thấp, JNI lỗi, TTS lỗi/voice không khả dụng
Lỗi minh bạch, không đổi gameplay; khôi phục an toàn.
Lệnh trực tiếp, thay nhiều người, lệnh trùng, hết quyền thay
UI và Engine đồng nhất accepted/pending/applied/rejected; không lẫn phiên.
Hiệp phụ, luân lưu, hết hiệp
Đồng hồ/vòng đời/score đúng, không thêm 15 phút nghỉ ngoài đời, bàn luân lưu tách riêng.
12. Phần A: Kế hoạch tái cấu trúc nền tảng
Mỗi chặng có đầu vào, phạm vi, đầu ra, cổng chấp nhận và kế hoạch rollback độc lập. Tách hoàn toàn thay đổi loại R khỏi B/I. Không tăng PI 4.0.29 trong Phần A.
A-1. Baseline Freeze: đóng băng trước mọi thao tác xóa
1. Xác nhận mốc đã duyệt (ADR-001: khuyến nghị HEAD c2c392d8), giữ tag/commit bất biến cho cả 4.0.27 và 4.0.28.
2. Khóa toolchain và lockfile: tạo và commit rust/rust-toolchain.toml (ghim bản Rust cụ thể) và rust/Cargo.lock; ghi nhận phiên bản NDK (27.2.12479018), JDK (17), Gradle, compileSdk 35, targetSdk.
3. Tải hoặc tái tạo raw population JSON, log job, kết quả audit 904 trận trên chính commit baseline, danh sách seed và input đầy đủ kèm SHA-256 manifest.
4. Tạo golden per-seed theo Mục 11.3 và các mẫu checkpoint/replay (live, pass flight, tackle/loose recovery, goal celebration, pending tactics, halftime, period end).
5. Chốt inventory đầy đủ: attribute_contracts.json (50 in-match / 4 career-only), danh sách 34 tệp Rust legacy + 9 hàm JNI legacy, 10 workflow, 11 tệp legacy trong verify_lab_v2.py, và 48 OGG + 895 template đã xác minh.
Cổng: cùng commit tái lập exact golden trên toolchain đã khóa. Rollback: chưa xóa mã; tag mốc nguyên vẹn.
A0. Phân loại và làm xanh CI có ý nghĩa
1. Sửa test pi4_legacy_api_and_direct_policy_match (v4/tests.rs:2691) theo đúng hợp đồng version/compatibility (loại T), không chỉ đổi chuỗi cứng mà kiểm tra cả manifest và negative case.
2. Xử lý 5 check P1 trong verify_v4_p1_234.py: thay 3 check beat lỗi thời bằng test hành vi DES thực tế; xác minh và sửa 2 mục còn giá trị nghiệp vụ (chọn thay nhiều người trong LabReferenceMatchActivity.kt và chống ghi đè archive trong LabGlobalSeedReplayStore.kt nếu thiếu — tách commit I riêng).
3. Sửa tools/validate_v4_p1_full.sh để fail cứng khi thiếu cargo/kotlinc.
4. Lập coverage map và hợp nhất 10 workflow hiện có (build-apk.yml, ci-runner-recovery-probe.yml, diagnose-v4-des.yml, pi-attribute-audit.yml, pi4-extraction-parity.yml, validate-des-repair.yml, validate-port.yml, validate-v4-android.yml, validate-v4-p1.yml, validate-v4-p2.yml) về kiến trúc 5 tầng L0–L4, trỏ toàn bộ sang rust-toolchain.toml và --locked.
5. Chạy đủ Rust --all-targets, 1.396 population, audit 54 trên mốc chọn và build APK ARM64.
Cổng: required gates baseline refactor xanh toàn bộ; không còn workflow đỏ chưa phân loại. Rollback: revert riêng từng commit T/I.
A1. Loại bỏ legacy không còn được sử dụng
Mục tiêu: chỉ còn một engine gameplay 4.x trong production.
• Rust: xóa 34 tệp legacy ở rust/football_match_engine/src/ (đã xác nhận v4/ không import bất kỳ tệp nào trong 34 tệp này), giữ 5 tệp chia sẻ/biên (clock.rs, fingerprints.rs, rng.rs, lib.rs, jni_bridge.rs). Xóa unit tests V3 trong lib.rs. Gỡ đủ 9 hàm JNI legacy (6 hàm unsafe trong jni_bridge.rs + 3 hàm nativePortVersion, rngSample, nativeCoreSelfCheck trong lib.rs).
• Android: gỡ LabEngineVersion.V3 và nhánh thực thi cũ ở SimulationSettingsActivity.kt, SimulationSettingsStore.kt, LabMainActivity.kt, LabModels.kt, LabMatchInputFactory.kt (bao gồm project54ForV3), LabRustLiveSession.kt, LabReferenceMatchActivity.kt, NativeBridge.kt. Migrate preference cũ an toàn, không crash.
• Tools & CI (Cập nhật đồng thời trong cùng PR): cập nhật hoặc thay thế tools/verify_lab_v2.py (gỡ 11 assert tệp Rust legacy ở root để không làm vỡ build-apk.yml), lưu trữ/gỡ phần chạy legacy trong tests/parity/, tools/run_v2_parity.sh, tools/verify_parity_fixtures.py sau khi đã archive.
• Tài liệu: chuyển các tài liệu kiến trúc/calibration/PI cũ sang docs/archive/ có chỉ mục, giữ AUDIO_SHA256_MANIFEST.txt, AUDIO_LICENSES.md, docs/ENGLAND_MINIWORLD_V2.md, docs/COMMENTARY_EVIDENCE_POLICY.md làm tài liệu dữ liệu/chính sách phụ trợ. README.md trỏ về docs/FOOTBALL_SIMULATION_4_SPEC.md.
Cổng: Rust/Android/APK build xanh, 0 active legacy gameplay entry, exact parity trên golden. Rollback: tách từng nhóm commit xóa (Rust, Kotlin, Tools/CI, Docs) để dễ revert độc lập.
A2. Contracts, đơn vị và VersionManifest
Tách DTO/ID/geometry/attrs/errors/schema và các trait cổng (SpatialForecastPort, RngSampleSource) thành boundary nội bộ trước khi tách crate. Xây VersionManifest4 tách build vs behavior vs schema; kiểm thử tương thích checkpoint schema v2 hiện hành. Giữ nguyên phép toán, số float, ordering và ABI.
Cổng: schema roundtrip, unknown/invalid payload tests, exact parity và checkpoint restore.
A3. Cắt biên dữ liệu PI và Coach
1. Tạo CompatibilityObservation hẹp và nối player_intelligence qua SpatialForecastPort + RngSampleSource do Engine cung cấp; gỡ toàn bộ import MatchStateV4, MatchInputV4, v4::ball_play, v4::movement, v4::interaction, v4::rules, DeterministicRng trực tiếp khỏi player_intelligence/ mà vẫn giữ nguyên thứ tự tính toán và RNG. Ghi ADR danh sách trường ngoại lệ tạm thời cần đóng ở B2.
2. Tách v4/ai_manager.rs: chuyển apply_manual_substitution_group về biên Engine/Rules; bọc evaluate_ai_managers qua adapter tương thích cho Coach.
3. Dọn v4/perception.rs sau khi cập nhật call site trong v4/checkpoint.rs và test.
Cổng: isolated policy output parity, FullEvent/golden exact, negative test cổng dữ liệu. A3 chưa được gắn nhãn «AI hết toàn tri».
A4. Phân chia World State và làm mỏng điều phối
Tách các nhóm transition (Physics facts/delta, Rules decisions/restart, Condition, Tactics, PI/Coach intents, Telemetry) trong khi Engine giữ authoritative MatchState và đường commit nguyên tử. Rút gọn v4/session.rs theo quyền sở hữu. Đưa checkpoint của từng subsystem thành payload rõ. Giữ nguyên thứ tự floating point, RNG và event priority.
Cổng: unit/integration, invariants INV-01..12, no half-commit, exact parity và checkpoint continuation.
A5. Cargo Workspace: tách từng crate có thứ tự
Thiết lập rust/Cargo.toml workspace với 10 crate production và rust/tests/ integration harness (di chuyển v4/tests.rs phần tích hợp, v4/pi_parity_fixture.rs, v4/calibration.rs, v4/attribute_impact_tests.rs, và sửa test p2_50_match_attributes... khỏi include_str! đường dẫn cứng).
Thứ tự tách từng crate: match_contracts → player_condition → match_rules → match_physics → match_tactics → player_intelligence → coach_intelligence → match_telemetry → match_engine → football_match_engine_jni (đặt [lib] name = "football_match_engine"). Mỗi crate sau khi tách phải qua build, test riêng, cargo metadata DAG gate và golden subset; hoàn tất vòng lặp chạy full golden.
Cổng: workspace --all-targets --locked PASS, cargo metadata DAG sạch, exact parity 100%, không tạo crate rỗng.
A6. JNI và Android Presentation boundary
1. Cập nhật tools/build_lab_rust_android.sh và workflow L4 trỏ sang crate football_match_engine_jni trong workspace, xác minh xuất đúng libfootball_match_engine.so (ARM64) và 15 hàm V4/DES + manifest API.
2. Tách controller khỏi LabReferenceMatchActivity.kt, chuẩn hóa ranh giới package match_playback, match_commentary, match_audio.
3. Cập nhật task validateCommentaryTokens trong app/build.gradle nếu đường dẫn/cấu trúc LabCommentaryEngine.kt thay đổi; không để vỡ preBuild.
4. Giữ nguyên semantics lookahead/rate/commentary hiện hành trong commit R; áp dụng quyết định targetSdk (nếu có theo ADR-009) ở commit I riêng.
Cổng: Kotlin/JVM tests, JNI contract, :app:validateCommentaryTokens, APK ARM64, parity Rust và event/cue trace.
A7. Nghiệm thu nền tảng tái cấu trúc
Chạy lại toàn bộ 1.396 trận khóa, 12 FullEvent trace, command scenarios, FullEvent/FastBatch parity, checkpoint exact continuation, audit 54, cargo metadata DAG, Rust/Kotlin/APK và benchmark trên môi trường khóa. Cập nhật ADR/evidence và liệt kê nợ hành vi chuyển sang Phần B.
Cổng: 0 sai khác gameplay không phê duyệt, toàn bộ automated gates L0–L4 xanh. Không tuyên bố release thiết bị ở A7.
A8. Nghiệm thu Android thật và chốt mốc Phần A
Chạy ma trận thiết bị thật Mục 11.5 trên thiết bị mục tiêu theo ngưỡng ADR-009; kiểm tra APK hash, commit, drift, backlog, nhiệt, pin, AudioFocus, force-stop/restore. Nếu chưa có thiết bị, giữ trạng thái A8 BLOCKED/Chờ xác minh; chủ dự án có thể duyệt cho phép nghiên cứu offline Phần B theo ngoại lệ ADR, nhưng không công nhận release.
13. Phần B: Lộ trình phát triển từ gốc lên ngọn
Bắt đầu sau khi A7 đạt cổng (và A8 hoàn tất cho mọi hạng mục phát hành/trình diễn). Mỗi nâng cấp tăng behavior ID đúng crate thay đổi trong họ 4.x.y.
B1. Tầng 1: Physics + Rules + Condition
Kiểm kê và chứng minh cơ chế hiện có trước khi sửa; mỗi hạng mục có baseline, giả thuyết, test cô lập và quần thể riêng:
Mã
Phạm vi
Cơ chế và tiêu chí nghiệm thu
PHY-01
Đơn vị, hệ tọa độ, thời gian
Chốt kích thước/biên, frame tấn công, đơn vị, priority, canonical math, invariant. Mirror và dt test.
PHY-02
Vận động cầu thủ
Thống nhất gia tốc, giảm tốc, đổi hướng, reaction và tốc độ hiệu dụng cho mọi pha. Kiểm tra distance/time budget.
PHY-03
Thể lực, chấn thương, tâm lý nền
Tích phân một lần theo chuyển động thật, hai bể năng lượng, recovery, injury exposure; phân tích 14,6 km baseline.
PHY-04
Bóng 3D
Lăn, bay, nảy, drag, gravity, biên/khung thành; tính từ trajectory, không gán bóng vào người nhận.
PHY-05
Cầu thủ–bóng
Tách launch/flight/contact/control, race đối xứng, sai số phát bóng, khống chế, cập nhật dự báo khi motion đổi.
PHY-06
Cầu thủ–cầu thủ
Hình học tiếp xúc, tì đè, che chắn, tắc bóng, cản người, va chạm thân thể, hụt bóng; tách contact facts và foul decision.
PHY-07
Bóng hai
Hướng/động lượng bật ra tương đối, nhiều người tranh chấp, boundary trước contact; không ép recovery 50/50.
PHY-08
Không chiến
Nhảy, độ cao, điểm chạm, heading/clearance, va chạm và luật (Jumping Reach, Heading, GK Aerial Reach).
PHY-09
Thủ môn
Vị trí, lao ra, reach, reaction, bắt/đẩy/đấm, one-on-one, phân phối (Handling, Reflexes, Punching, Throwing).
PHY-10
Bóng chết và thể thức
Restart lifecycle đầy đủ, ném biên/phát bóng/corner/free kick/penalty, lợi thế, thẻ, bù giờ, hiệp phụ/luân lưu.
PHY-11
Mặt sân, thời tiết
Tác động cơ học có căn cứ, không cộng penalty trùng lặp.
PHY-12
Nghiệm thu tích hợp
Invariants, audit 54, quần thể cân bằng/mirror/sức mạnh/luật/chiến thuật, benchmark và rollback.
Song song: RULE-01..04 (ruleset/restart, offside/advantage, foul/card/penalty theo facts, period/added time/shootout) và COND-01..03 (tải vận động, injury exposure, tâm lý).
Điều tra 4 bất cập tồn đọng từ 4.0.27/4.0.28:
1. Phạm lỗi thấp: phân rã opportunities → commits → reached contacts → body contact → foul decision → advantage/stoppage; so sánh 4.0.27 vs 4.0.28.
2. Bóng sống 81 phút: phân rã loại restart, số lần, thời lượng chuẩn bị, sự kiện biên/foul/offside/goal bị bỏ qua.
3. Vận động ~14,6 km: tách distance do target churn/policy đuổi bóng khỏi mô hình vận động vật lý; sửa movement trước stamina.
4. Chênh lệch trình độ cực đoan (16 vs 9 tạo 13–14 bàn): chạy gradient & factorial theo từng nhóm thuộc tính, tìm điểm khuếch đại lặp (fatigue/skill/confidence/utility); cấm handicap hoặc cap tỷ số.
B2. Tầng 2: Player Intelligence trên thế giới đã kiểm chứng
Mã
Nâng cấp
Điều kiện chấp nhận
PI-01
Observation hữu hạn
Đóng toàn bộ compatibility fields còn lại từ A3, field access audit, hidden-world equivalence.
PI-02
Nhận thức thời gian và dự đoán
Độ trễ, uncertainty, dự báo chuyển động/khoảng trống không đọc RNG outcome.
PI-03
Giá trị hành động chung
So sánh chuyền/sút/dẫn/giữ/phá trên cùng thang lợi ích/rủi ro/chi phí/turnover.
PI-04
On-ball và timing
Chuẩn bị phát bóng, một chạm, re-aim/re-evaluate không trùng dt tích phân.
PI-05
Chạy chỗ và chuyền khoảng trống
Dự đoán hướng chạy, chuyền đón, nhận thức bẫy việt vị, tạo góc hỗ trợ.
PI-06
Defensive intelligence
Tắc/kiềm chế theo geometry, thẻ, cover, threat; đánh giá lại utility 4.0.28 trên Physics mới.
PI-07
Goalkeeper decision
Vị trí, lao ra hay giữ, bắt/đẩy, phân phối (tách khỏi thực thi vật lý).
PI-08
Memory, personality, adaptation
Trí nhớ có decay, học từ điều đã quan sát, cá tính không tạo bonus toàn tri.
PI-09
Nghiệm thu
Isolated scenarios, forecast calibration, 54 attributes theo ngữ cảnh, paired population, no-omniscience.
B3. Tầng 3: Match Tactics và phối hợp tập thể
Hoàn thiện role/duty theo 4 pha, tổ chức chuyển trạng thái, phối hợp pressing/screen/cover, hỗ trợ góc chuyền, chiều rộng/độ sâu, overload, bóng chết, độ trễ lan truyền và phản ứng khi thiếu người. Kiểm chứng bằng ma trận chiến thuật (High Press vs Low Block, Mid Block vs Direct Play, tempo/risk, biên/trung lộ, mirror) đo riêng cho từng đội; không tạo cơ chế «đảo kết quả» để ép tỷ lệ thắng cân bằng.
B4. Tầng 4a: Coach Intelligence
Chuyển hóa planOptimalLineup và buildAutoSetPiecePlan (hiện ở LabTacticalPresets.kt) cùng chính sách scouting hợp lệ thành PreMatchPlan trong Rust coach_intelligence. Hoàn thiện HLV trong trận (điều chỉnh chiến thuật, thay người, xử lý chấn thương/thẻ đỏ/time management) đi qua chung MatchCommand queue và validation với người chơi; kiểm tra không oscillation và không đọc tương lai.
B5. Tầng 4b: Commentary 4.0, Audio 4.0 và Speculative Lookahead (nếu duyệt ADR-006)
Xây narrative từ sự kiện đã commit, template song ngữ có ngân sách timeline chuẩn, seed biến thể độc lập, FIFO 1.0x–3.0x không auto mute, ambient/SFX theo context sân/khán đài. Nếu ADR-006 phê duyệt speculative rollback, triển khai và nghiệm thu toàn bộ ca rollback/generation/audio trong giai đoạn này (sau khi nền tảng A7/A8 đã vững).
B6. Nghiệm thu hệ sinh thái
Đạt toàn bộ cổng automated (L0–L4), device (L5), compatibility và evidence. Công bố phiên bản module thực tế, behavior IDs, ruleset, limitations, dataset nguồn và benchmark; chốt tag phát hành trong họ 4.x.y.
14. Quản trị thay đổi, rủi ro và quyết định cần phê duyệt
14.1. Quy trình thay đổi
Mỗi issue/PR ghi rõ ID, loại R/B/I/T/D, crate owner, baseline SHA, mục tiêu/giả thuyết, scope không làm, test plan, version impact, evidence và rollback plan. Không gộp xóa legacy, đổi công thức, đổi ABI và cập nhật golden trong một commit. Không force-push xóa lịch sử lỗi. Tài liệu cũ lưu ở docs/archive/; bằng chứng CI quan trọng lưu retention phù hợp kèm SHA-256.
14.2. Risk register
Rủi ro
Dấu hiệu
Phòng ngừa và xử lý
R1 Mất baseline / trôi toolchain
Không tái lập được golden; không cóCargo.lock
Khóa Cargo.lock + rust-toolchain.toml ở A-1 trước mọi thao tác xóa; lưu SHA-256 manifest.
R2 Refactor đổi hành vi
First divergence xuất hiện
Đối chiếu từng crate, giữ thứ tự RNG/float, revert nhóm gây lỗi.
R3 Contracts thành monolith
Công thức Physics/Rules lọt vàomatch_contracts để né vòng phụ thuộc PI
Dùng trait port hẹp (SpatialForecastPort,RngSampleSource) hoặc ngoại lệ DAG có thời hạn đến B2; review DAG bằngcargo metadata.
R4 AI toàn tri qua DTO
Đổi thuộc tính ẩn làm đổi quyết định dù observation giống nhau
Negative test hidden-world equivalence, đóng ngoại lệ tại B2/B4.
R5 CI xanh giả / vỡ build dây chuyền
validate_v4_p1_full.sh skip ngầm;verify_lab_v2.py hoặcvalidateCommentaryTokens làm đỏ build sau A1/A6
Hợp nhất 10 workflow về L0–L4, fail cứng khi thiếu tool, cập nhật script & Gradle task cùng PR.
R6 Checkpoint không tương thích
Crash/restore sai hoặc mất pending action
VersionManifest4, schema validation, AtomicFile + backup generation.
R7 Lookahead lộ tương lai / mất điều khiển
UI/TTS báo trước bàn; command bị trễ giả
Giữ nguyên frontier an toàn trong A, triển khai rollback ở B5 theo ADR-006.
R8 Drift/TTS quá tải
Cue tồn đọng, đọc chồng, auto mute
Budget timeline 1.0x, đo thiết bị thật A8, báo overload minh bạch.
R9 Khuếch đại năng lực
Tỷ số cực đoan 13–14 bàn ở chênh lệch cao
Gradient/factorial theo cơ chế ở B1, cấm cap score.
R10 Cross-platform lệch bit
Linux x86_64 và Android ARM64 divergence
Khóa toolchain, float_roundtrip, đối chiếu cross-target trace; chưa đạt giữ Chờ xác minh.
R11 Chi phí hiệu năng / lệchtargetSdk
3.0x underflow, RAM/nhiệt tăng, lỗi AudioFocus do targetSdk 29
Benchmark môi trường khóa, chốt lộ trình targetSdk ở ADR-009.
R12 Thiếu thiết bị/dữ liệu thật
Không có device report hoặc chuẩn thống kê có nguồn
Giữ BLOCKED/Chờ xác minh, không giả log.
R13 Mở rộng scope
Tạo crate/feature career chưa có input/test
Giữ chặt phạm vi 4.0, không tạo placeholder.
14.3. Các quyết định cần chủ dự án phê duyệt (ADR)
ID
Khuyến nghị chốt
Nội dung cần chủ dự án xác nhận
ADR-001
Chọn HEAD 4.0.28 (c2c392d8) làm parity baseline (giữ tag 4.0.27); sửa 1 dòng version test ở A0 và chạy audit 904 trận trên 4.0.28
Xác nhận chọn 4.0.28 thay vì revert về 4.0.27.
ADR-002
Dùng nhánh tích hợp refactor, main chỉ nhận PR đã xanh L0–L4
Xác nhận dừng push trực tiếp main trong suốt Phần A.
ADR-003
Archive tài liệu/evidence cũ vàodocs/archive/ + tag bất biến + SHA-256 manifest
Xác nhận vị trí lưu và thời gian giữ artifact CI lớn.
ADR-004
10 crate production + trait port hẹp trongmatch_contracts(SpatialForecastPort,RngSampleSource) cho PI; giữ [lib] name = "football_match_engine"
Xác nhận dùng trait port (hoặc cho phép ngoại lệ DAG tạm đến B2).
ADR-005
Tách BuildIdentity,GameplayIdentity, DataSchemas; từ chối rõ checkpoint không tương thích
Xác nhận chính sách dữ liệu save cũ và event replay legacy.
ADR-006
Giữ semantics lookahead hiện hành trong Phần A; hoãn speculative rollback sang B5 (pause khi mở dialog)
Xác nhận không đưa rollback vào phạm vi Phần A.
ADR-007
Cùng nội dung cue ở mọi rate, FIFO, không auto mute; khi quá tải giữ clock gameplay và báo backlog minh bạch
Xác nhận chọn đọc nốt sau trận hay pause presentation khi TTS trễ.
ADR-008
Bit-exact trên cùng toolchain đã khóa (rust-toolchain.toml + Cargo.lock); kiểm chứng Linux vs ARM64 riêng
Xác nhận có bắt buộc bit-exact cross-platform trước release 4.0 hay không.
ADR-009
Khóa device matrix, ngưỡng drift/RAM/nhiệt/pin và quyết định giữ targetSdk 29 hay nâng lên 34/35
Chốt thiết bị mục tiêu, ngân sách tài nguyên vàtargetSdk.
ADR-010
Dải thống kê hiện tại chỉ là tham khảo; xây bộ chuẩn realism có nguồn giải đấu/mùa rõ ràng
Xác nhận nguồn dữ liệu tham chiếu cho Phần B.
ADR-011
Ruleset định danh theo thể thức (số lượt/cửa sổ thay người, hiệp phụ, luân lưu, bù giờ)
Xác nhận danh sách thể thức hỗ trợ trong 4.0.
15. Phụ lục: mẫu hợp đồng và biểu mẫu nghiệm thu
15.1. Giao diện khái niệm
Mã minh họa kiến trúc đích, không phải API đã tồn tại. Phần A bảo toàn hành vi legacy; Phần B tăng behavior ID khi đổi semantics.
// match_contracts: dữ liệu có kiểu và trait port hẹp; không chứa công thức nghiệp vụ.
trait SpatialForecastPort {
    fn estimate_pass_reach(&self, query: &PassReachQuery) -> PassReachForecast;
    fn evaluate_local_context(&self, query: &LocalContextQuery) -> LocalContextView;
    fn estimate_tackle_geometry(&self, query: &TackleGeometryQuery) -> TackleGeometryForecast;
    fn nominal_flight_ms(&self, kind: BallFlightKind, distance_m: f32) -> DurationMs;
}

trait RngSampleSource {
    fn next_unit_f32(&mut self) -> f32;
    fn next_int(&mut self, bound: i32) -> i32;
}

struct PlayerObservation {
    observed_at_ms: SimTimeMs,
    self_state: SelfObservation,
    ball: ObservedBall,
    teammates: Vec<ObservedPlayer>,
    opponents: Vec<ObservedPlayer>,
    assignment: TacticalAssignment,
    context: ObservedMatchContext,
}

struct DecisionPlan {
    intent: ActionIntent,
    decision_delay_ms: DurationMs,
    preparation_delay_ms: DurationMs,
    valid_until_ms: SimTimeMs,
    memory_delta: PlayerMemoryDelta,
}

struct PhysicalResolution {
    facts: Vec<ContactFact>,
    delta: PhysicalDelta,
    next_interaction: Option<InteractionPrediction>,
    activity: Vec<ActivitySample>,
}

struct RuleResolution {
    decision: RuleDecision,
    restart: Option<RestartSpec>,
    discipline: DisciplineDelta,
}

struct CommandReceipt {
    command_id: CommandId,
    requested_at_ms: SimTimeMs,
    accepted_at_ms: Option<SimTimeMs>,
    effective_at_ms: Option<SimTimeMs>,
    status: CommandStatus,
    reason: Option<CommandReason>,
}

struct VersionManifest4 {
    build: BuildIdentity,
    schemas: DataSchemas,
    gameplay: GameplayIdentity,
    config: MatchConfigIdentity,
    compatibility_id: String,
    presentation: Option<PresentationIdentity>,
}

Engine.handle(next_event):
    validate_generation_and_time
    stage_world_advance_and_rng
    resolve_physics_facts
    adjudicate_rules
    compute_condition_and_tactical_deltas
    validate_authoritative_invariants
    stage_scheduler_changes_and_command_receipts
    commit_transition_atomically
    publish_committed_event_and_telemetry

15.2. Mẫu hồ sơ baseline / evidence
Experiment ID:
Change type: R / B / I / T / D
Parent commit / candidate commit:
Baseline tag, behavior IDs, schema and fingerprint version:
Toolchain / rust-toolchain.toml / Cargo.lock hash / target / profile:
Ruleset / timing / dataset / input manifest hash:
Seed list and cohort sizes:
Command log and mode:
Completed / failed / invalid / ignored:
Raw artifact paths and SHA-256:
Parity projection and first divergence (if any):
Mechanism hypothesis and isolated tests:
Population paired differences and uncertainty:
Attribute audit exposure and results:
Performance and Android device evidence:
Compatibility changes and rollback:
Known limitations / blockers:
Decision: ACCEPT / REJECT / BLOCKED
Reviewer and approval:

15.3. Checklist bắt buộc trước mỗi PR/commit được hợp nhất
• [ ] Đã xác nhận baseline SHA, Cargo.lock/rust-toolchain.toml và phân loại R/B/I/T/D, không gộp refactor với đổi hành vi.
• [ ] Quyền sở hữu trạng thái và DAG (cargo metadata) đúng; PI/Coach không đọc state thật ngoài ngoại lệ có ADR trong Phần A.
• [ ] Không nắn thống kê, không quota giả, không dùng Telemetry điều khiển gameplay.
• [ ] Mọi thay đổi timing, RNG, behavior và schema có version/compatibility tương ứng.
• [ ] Test cô lập đúng cơ chế; test cũ còn giá trị không bị xóa/nới ngưỡng vô căn cứ.
• [ ] Đã chạy cargo test --workspace --all-targets --locked và kiểm tra cấu trúc L0/L1.
• [ ] Refactor có exact parity, FullEvent/FastBatch, checkpoint và first divergence report trống.
• [ ] Behavior có paired Monte Carlo, audit 54, báo cáo exposure, phân bố và tác động ngoài ý muốn.
• [ ] Android/JNI có build ARM64 .so (libfootball_match_engine.so), :app:validateCommentaryTokens và test JVM; phần chưa chạy thiết bị ghi Chờ xác minh.
• [ ] Event, command, checkpoint, replay, commentary và audio tuân hợp đồng; không lẫn match ID/generation.
• [ ] Benchmark trong budget đã duyệt; không skip gameplay/cue vì quá tải.
• [ ] Evidence, input, seed, toolchain, log và checksum truy xuất được; tài liệu/ADR cập nhật.
• [ ] Rollback khả thi; không còn blocker chưa phân loại; approval đã ghi.
15.4. Mẫu nghiệm thu mỗi chặng
Hạng mục
Evidence cần có
Trạng thái
Kiến trúc
cargo metadata DAG, ownership diff
Chưa chạy / PASS / FAIL / BLOCKED
Hành vi
Golden parity hoặc paired behavioral report
Chưa chạy / PASS / FAIL / BLOCKED
Tính tất định
Clock, RNG, state, checkpoint, mode
Chưa chạy / PASS / FAIL / BLOCKED
Kiểm thử
CI URL, số pass/fail/ignored, gate coverage
Chưa chạy / PASS / FAIL / BLOCKED
Tương thích
VersionManifest4, migration/reject test
Chưa chạy / PASS / FAIL / BLOCKED
Android
APK hash, .so ELF check, integration/device evidence
Chưa chạy / PASS / FAIL / BLOCKED
Hiệu năng
Benchmark với môi trường khóa
Chưa chạy / PASS / FAIL / BLOCKED
Giới hạn
Issue ID, owner, điều kiện đóng
Ghi rõ
15.5. Nguồn và lịch sử revision
• Revision 1: Văn bản gốc 239 dòng ngày 25/11/2026 xác lập tầm nhìn, 13 module và khung tái cấu trúc.
• Revision 2: Mở rộng hợp đồng 15 nguyên tắc, 12 invariant, quy trình parity, CI nhiều lớp, ma trận nghiệm thu thiết bị và ADR-001..011.
• Revision 3 (Bản này): Đối chiếu trực tiếp toàn bộ kho mã nguồn tại commit c2c392d8b3ccc57985e87337e8994fd7033328ee; hiệu đính danh mục 9 hàm JNI legacy (6 ở jni_bridge.rs, 3 ở lib.rs), ánh xạ đầy đủ v4/perception.rs, v4/pi_parity_fixture.rs, v4/tests.rs và tách đúng ranh giới v4/ai_manager.rs / LabTacticalPresets.kt; bổ sung giải pháp trait port (SpatialForecastPort, RngSampleSource) xử lý phụ thuộc thực tế của player_intelligence; bổ sung khóa Cargo.lock + rust-toolchain.toml ở A-1; xử lý trước các bẫy vỡ build (verify_lab_v2.py, validateCommentaryTokens, tên .so, validate_v4_p1_full.sh, targetSdk 29); cập nhật trạng thái xác minh 48 OGG + 895 mẫu bình luận + 50/4 phân loại 54 thuộc tính và làm rõ nguyên nhân đỏ CI ở 4.0.28 (v4/tests.rs:2691) cùng 5 check P1.
• Bằng chứng CI tham chiếu: 4.0.27 DES 45520666910, audit 4.0.27 45520666555, P1 45520666671, 4.0.28 DES 45527962031, baseline 4.0.28 138915898432, lỗi version on-ball 4.0.28 138915898475.
Tuyên bố kết thúc
Sửa nguyên nhân, không sửa con số kết quả.
Hệ sinh thái 4.0 phải có một thế giới nhất quán, một nguồn sự thật gameplay, những cầu thủ có giới hạn nhận thức, HLV công bằng, chiến thuật có đánh đổi, và tầng trình diễn trung thực. Tái cấu trúc chỉ thành công khi ranh giới không còn là tên thư mục mà là quyền hạn, hợp đồng dữ liệu và bằng chứng được cưỡng chế. Phát triển chỉ thành công khi mỗi thay đổi có thể giải thích, kiểm chứng và hoàn tác.
Bước đầu tiên sau khi chốt ADR và đưa tài liệu này vào docs/FOOTBALL_SIMULATION_4_SPEC.md: A-1 Baseline Freeze (tạo rust-toolchain.toml + Cargo.lock và khóa golden). Không xóa mã, không tiếp tục PI 4.0.29 trước khi mốc đối chứng đã được bảo toàn.