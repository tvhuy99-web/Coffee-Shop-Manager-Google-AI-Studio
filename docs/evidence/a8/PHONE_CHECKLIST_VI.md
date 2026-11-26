# A8 — Phiếu kiểm thử điện thoại (PI 4.0.28)

Ứng viên cố định: e540c51a675dce30f878b01a211262740608274b
CI 45747583976: 19/19 PASS. 1.384/1.384 trận cross-mode exact; 12 FullEvent và checkpoint markers golden PASS.
APK artifact: https://github.com/tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio/actions/runs/45747583976/artifacts/15083733591
SHA-256: 6f6b818e4ff42d396cedd052290283bd283b437e7dee3fabe135ac3cac5aa865
Package: pl.mkrstudio.truefootball3.debug (Debug, ARM64). Cùng byte APK f84c13f.
Không cần xóa ứng dụng release, không xóa dữ liệu hiện có. Không gửi IMEI, serial, số điện thoại, tài khoản, log cuộc gọi.

## Thông tin thử
- Model điện thoại:
- Phiên bản Android:
- RAM (nếu biết):
- Voice TTS / ngôn ngữ:
- Mức pin đầu/cuối (nếu tiện):
- Seed, đội nhà/đội khách, mode, rate:

## Vòng 1 — trải nghiệm nhanh
Ghi PASS, FAIL, NOT_RUN; nếu FAIL ghi phút, thao tác, hiện tượng, seed nếu có.
1. Cài đặt, mở app, chọn trận, bắt đầu Full Match.
2. Chạy 1x, 2x, 3x trong cùng trận, đổi tốc độ giữa câu; cue không mất/lặp do đổi rate, tỷ số và đồng hồ không nhảy lùi; quan sát backlog nếu có.
3. Pause/resume giữa câu và giữa trận.
4. Đổi chiến thuật, thay nhiều cầu thủ cùng đợt; không thay trùng, đúng roster và trạng thái lệnh.
5. Về Home, khóa màn hình, mở lại, đổi audio focus bằng ứng dụng âm thanh khác; ghi cách app hành xử, đối chiếu chính sách lifecycle.
6. Sau khi đã có checkpoint hợp lệ, buộc dừng riêng ứng dụng debug (Cài đặt > Ứng dụng > PingFootball debug > Buộc dừng), KHÔNG xóa dữ liệu; mở lại, xác nhận cùng trận, thời điểm lưu và tỷ số. Có thể mất phần diễn biến sau checkpoint cuối; không được mất cả trận, không đếm bàn/cue lặp.
7. Chạy hết trận, xác nhận tỷ số cuối, thống kê, lưu replay.
8. Thử Highlights: bàn thắng, phản lưới, kết thúc hiệp/trận không bị lọc mất.

## Vòng 2 — đầy đủ theo spec 11.5
Chạy cùng seed, cùng đội, cấu hình, mode, không lệnh thủ công ở 1x, 2x, 3x riêng (dùng replay/seed nếu giao diện hỗ trợ). So sánh kết quả và cue; không cần bắt người dùng chờ đủ ba trận ngay vòng đầu.
- Force-stop tại: bóng sống, goal celebration, corner/free kick, halftime, pending command, giữa TTS hoặc ghi checkpoint. Ghi chính xác checkpoint được khôi phục. Không xóa dữ liệu.
- Thử lệnh trùng, hết quyền thay, lệnh bị từ chối; hiệp phụ, luân lưu và kết thúc hiệp.
- Bộ nhớ thấp, JNI/TTS lỗi: chỉ dùng điều kiện thử an toàn; không sửa điện thoại, không cố gây crash hệ thống. Ca không tái hiện được ghi NOT_RUN.
- Đo drift/backlog, CPU/RAM, pin, nhiệt, frame, JNI latency bằng công cụ/ADB. Nếu không có PC, vòng 1 vẫn hữu ích nhưng không thay L5.

## Mẫu gửi kết quả nhanh
Máy / Android:
APK e540c51 (hoặc f84c13f giống hệt):
1. Cài + bắt đầu trận:
2. Rate + TTS/âm thanh:
3. Pause/resume:
4. Chiến thuật/thay người:
5. Home/khóa màn hình:
6. Buộc dừng/khôi phục:
7. Kết thúc và replay:
8. Highlights:
Lỗi (phút, seed, thao tác, hiện tượng):
Các ca chưa thử:

## Chặn nghiệm thu
PASS trải nghiệm thủ công chỉ là dữ liệu quan sát. A8 L5 vẫn BLOCKED cho đến khi có thiết bị thật, kết quả đủ ma trận, bằng chứng đo và ngưỡng ADR-009 được duyệt. Không suy diễn từ CI Linux hoặc fake-adb. Giữ targetSdk 29 hiện hành, không tự nâng. Không phát triển B1–B6.
