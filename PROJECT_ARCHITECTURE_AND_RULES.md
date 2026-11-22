# Kiến trúc chuẩn: Football Match Lab (giai đoạn chuyển đổi)

## Ranh giới trách nhiệm

| Thành phần | Trách nhiệm |
|---|---|
| Android Kotlin | Màn hình, lựa chọn 20 đội EPL, trình chiếu bình luận, TTS, âm thanh, cài đặt |
| Rust | Trạng thái trận đấu, chiến thuật, xử lý hành động, luật, thống kê, sự kiện mô phỏng |
| JNI | Giao tiếp Kotlin ⇄ Rust, không tự tạo hoặc sửa kết quả mô phỏng |
| `tests/parity/` | Fixture *đã lưu* từ engine Java V3 gốc, phục vụ đối chiếu khi kiểm thử Rust |

## Các nguyên tắc bắt buộc

1. **Giai đoạn 1 là chuyển đổi, không cải tiến công thức hoặc đổi chỉ số.** Sử dụng
   thuộc tính PingFootball V3 và dữ liệu 20 đội EPL nguyên bản.
2. Màn hình **Cài đặt TTS** và **Cài đặt âm thanh** độc lập, không gộp.
3. Giữ nguyên tài nguyên 48 OGG và hai ngôn ngữ bình luận; không tự thêm sự kiện
   không phát sinh từ Match Engine để tạo lời tường thuật giả.
4. Không đưa các bộ dữ liệu giải đấu nhiều mùa vào Match Lab: đây là ứng dụng thử
   trận đấu đơn lẻ.
5. Kotlin chỉ thể hiện sự kiện; Rust là nguồn dữ liệu gameplay duy nhất.
6. Nếu thiếu thư viện JNI Rust, phải báo lỗi minh bạch, không tự chạy bằng Java.
7. Không tuyên bố parity chỉ vì script Python hoặc kiểm tra tài nguyên đạt: cần
   thực thi `cargo test` và so sánh sự kiện/fingerprint với fixture Java V3.
8. Không coi mã nguồn Kotlin/Rust có sẵn là APK có thể chạy nếu chưa build và test.

## Các việc đã hoàn tất trong lượt sửa lỗi này

- Sửa chuỗi xuống dòng trong `LabMainActivity.kt` (lỗi biên dịch Kotlin).
- Chuyển Gradle kiểm tra token bình luận từ Java đã xóa sang Kotlin hiện tại.
- Thay script parity phụ thuộc Java/script đã xóa bằng kiểm tra fixture + `cargo test`.
- Thêm script biên dịch JNI Rust bằng cargo-ndk và cập nhật CI, hướng dẫn.

## Điều kiện nghiệm thu giai đoạn 1 (chưa xác minh hết)

1. `cargo test --all-targets` trên Rust toolchain hoạt động, bao gồm parity đầy đủ.
2. JNI Rust `.so` biên dịch cho đúng ABI và đóng gói trong APK.
3. APK chạy trận thật, thống kê và lời bình luận phản ánh sự kiện Rust.
4. Màn hình cài đặt TTS/âm thanh, xem lại lời bình và TalkBack được thử trên Android.
5. Những sai lệch Java/Rust được ghi rõ, có kiểm thử hồi quy và xử lý trước nghiệm thu.

Sau khi có bằng chứng giai đoạn 1 đạt mới mở giai đoạn 2 dùng 54 thuộc tính mới.
