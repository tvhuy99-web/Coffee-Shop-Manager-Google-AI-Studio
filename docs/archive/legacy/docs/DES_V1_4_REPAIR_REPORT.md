# DES v1.4 — Repair pass 4 / v4.1.1-des

## Trạng thái
REPAIRED SOURCE CANDIDATE — RUNTIME VERIFICATION PENDING.
Bản ZIP là overlay mã nguồn, không phải repository hoàn chỉnh hay APK. Dựa trên ZIP Rework Pass 3 do chủ dự án cung cấp. Không có thay đổi trên GitHub/main.

## Những thay đổi đã thực hiện
- Rust: loại bỏ giải quyết lại toàn bộ đường chuyền/cú sút khi đến đích bằng cách lấy mẫu một lần khi phát bóng, lưu trajectory, mốc đến, kết quả, delta thống kê và trạng thái bóng; áp dụng tại mốc scheduler. Bảo toàn snapshot việt vị tại lúc phát bóng. EarlyInteractionResolved dùng mốc tương tác. Không sao chép đè toàn bộ trạng thái thế giới từ bản sao lúc phát bóng. Đây vẫn là mô hình dự đoán tương tác tại thời điểm phát bóng; độ chính xác khi cầu thủ đổi hành vi giữa đường bay cần kiểm chứng thêm.
- Rust: sửa các tên trường/enum không tương thích phát hiện trong rà soát; lưu pending flight, RNG, diagnostics và tăng phiên bản checkpoint. Cộng bù giờ từng phần theo số đã ghi nhận để tránh cộng trùng.
- Android: thống nhất đường tạo lời bình giữa trận mới và trận khôi phục; sửa khai báo, tên tham số, nullable receiver và ArrayDeque; bổ sung lựa chọn tiếp tục trận lưu. Tách tỷ số/trạng thái hiệp và cập nhật đội hình khỏi callback TTS; chặn kết thúc hiệp/trận sớm; không hủy sự kiện đã được Rust xác nhận khi thay đổi chiến thuật. Mở rộng lookahead khi Rust không thể tiến trong khung yêu cầu để giảm nguy cơ đứng trận lúc bóng chết dài.
- Checkpoint: lưu lại trạng thái trình bày, mốc bắt đầu hiệp, mốc biên, tỷ số, roster chờ, sự kiện/cue chưa phát; chặn bước native mới khi xuất checkpoint, xác minh mốc Rust/Android; khôi phục offset và ID lời bình. Schema mới không tương thích checkpoint DES cũ; không xóa bản lưu cũ tự động.
- TTS: FIFO theo cue đầu hàng đợi, bảo vệ WAV chưa đọc, giữ ID/nội dung khi đổi tốc độ; ghi nhận backlog ở ranh giới hiệp/trận. Giữ hướng hiệu chỉnh 4,2 âm tiết/giây của v3, chưa kiểm chứng bằng bộ đọc thật. Không mặc định tắt TTS ở 3x.
- QA: bỏ các kiểm tra giả chỉ so sánh hằng số hoặc bản sao danh sách trong Smoke.kt; thay bằng harness thực thi trực tiếp các lớp production Kotlin với dependency giả lập. Thêm workflow GitHub Actions để chạy Cargo, Monte Carlo, Kotlin, hồi quy bình luận và build APK ARM64.
- Đổi identity engine thành v4.1.1-des, vẫn giữ protocol/event schema 5; checkpoint schema 2. Không coi replay sinh từ bản DES chưa sửa là tương đương với bản sửa này.

## Bằng chứng đã thực thi tại đây
- python3 tools/verify_v4_des_contract.py: 26 PASS, 0 FAIL (kiểm tra tĩnh, không chứng minh runtime).
- bash tools/run_des_playback_jvm.sh: biên dịch thành công LabModels, MatchCheckpointStore, MatchPlaybackController, NarrationScheduler với kotlinc-jvm 1.9.0; 6 tình huống QA-03,04,05,06,07,09 PASS. Sử dụng fake clock, fake native session, fake TTS và registry JSON trong cùng process; không phải kiểm thử Android thật. Không xác nhận checkpoint thực trên file JSON sau khi tiến trình chết.
- ZIP test CRC: kiểm tra khi đóng gói.
Xem log tại repair-evidence/.

## Các cổng nghiệm thu chưa đạt hoặc chưa xác minh
- Cargo/rustc, Rust unit/integration tests, FastBatch parity, Monte Carlo 128/1000 trận: chưa chạy do môi trường hiện tại thiếu toolchain và không có bản repository đầy đủ. Mã Rust sửa theo rà soát tĩnh, có thể còn lỗi biên dịch hoặc hành vi.
- APK, JNI ARM64, Activity/Android/TTS MediaPlayer trên điện thoại, resume khi Android kill process: chưa build/chạy. Kotlin harness chỉ kiểm tra một tập lớp production với môi trường mô phỏng.
- Timeline 90 phút với âm thanh đầy đủ, chất lượng giọng 1x–3x và ngân sách lời bình ở các tình huống dày: chưa chứng minh. Việc báo backlog không đồng nghĩa đã giải quyết hết khả năng tồn đọng âm thanh tại hết giờ.
- Thời lượng, thống kê bóng đá và hiệu năng chưa hiệu chỉnh. Cần xác minh luật thay người AI, mốc tương tác bóng sống, tính nhất quán roster/thống kê tại presentation frontier, checkpoint đồng thời, và tương thích các kiểm thử CI có sẵn.
- Workflow mới chưa chạy trên GitHub. Không được tuyên bố PROJECT ACCEPTED hoặc CODE COMPLETE chỉ dựa vào ZIP này.

## Cách tích hợp an toàn
1. Lấy repository đầy đủ tại commit cơ sở tương ứng bản v3. Sao lưu thay đổi hiện tại; tạo nhánh riêng `feat/des-v1.4-repair`. Không áp dụng chồng một cách mù quáng nếu repository đã thay đổi.
2. Giải nén ZIP vào thư mục gốc repository, giữ nguyên cấu trúc `app/`, `rust/`, `tests/`, `tools/`, `.github/`, `docs/`. Gói chứa toàn bộ các tệp thay đổi/mới của v3 cộng các sửa chữa; không cần áp dụng patch cũ. Không xóa tệp ngoài gói.
3. Kiểm tra git diff, commit lên nhánh riêng. Chạy workflow `DES v1.4 repaired build and evidence` hoặc các lệnh tương ứng trên máy có Cargo, Gradle và Android SDK/NDK.
4. Sửa các lỗi CI nếu có; chạy thực tế Android, kiểm thử trận đủ thời lượng và TTS, save/restore, kiểm thử 1x/2x/3x; chỉ merge khi tất cả cổng bắt buộc đạt.
