# Football Match Lab 1.0

Ứng dụng Android Kotlin + Rust dành cho thử nghiệm mô phỏng trận đấu bằng dữ liệu PingFootball V3.
Giai đoạn 1 giữ nguyên công thức và chỉ số gốc, 20 đội Ngoại hạng Anh, TTS, bình luận và 48 âm thanh OGG.

- Phiên bản: `versionName 1.0`, `versionCode 1`.
- `main` chạy GitHub Actions để biên dịch Rust ARM64 với `--release`, kiểm thử parity, đóng gói **APK release được ký bằng khóa cố định**.
- Không sử dụng APK debug để đánh giá hiệu suất.
- Bắt buộc thiết lập GitHub Secrets tại repository này: `KEYSTORE`, `KEYSTORE_PROPERTIES`, `KEYSTORE_PASSWORD`. Hai giá trị đầu là payload AES-256-GCM được mở khóa trên runner bằng mật khẩu secret thứ ba; khóa rõ không nằm trong mã nguồn. Nếu thiếu, CI dừng và không phát hành APK không ký.
- APK thành công xuất hiện tại tab **Releases** và trong workflow artifact.
- Chú ý: app đã từng dùng versionCode 97. Bản mới có code 1 nên không thể cài đè; hãy sao lưu thiết lập trước khi gỡ bản cũ.

Xem [MATCH_LAB_README.md](MATCH_LAB_README.md) để biết cách cấu hình ký, cách build và các giới hạn kiểm thử.
