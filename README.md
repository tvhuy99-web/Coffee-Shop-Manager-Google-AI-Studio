# Cập nhật giai đoạn 2: dữ liệu bóng đá Anh

Đã đưa **English mini-world** lên `main`: 232 CLB, 11 giải/nhóm, 54 thuộc tính
mô phỏng, 5.640 cầu thủ đội một, 2.784 cầu thủ trẻ, 4.640 nhân viên và
4.780 lịch trận **giả lập**. File thử nghiệm cũ `epl20_original.json` đã bị xóa.

Đọc [hướng dẫn dữ liệu Anh V2](docs/ENGLAND_MINIWORLD_V2.md) để phân biệt
thông tin lấy từ bản nguồn PingFootball 2025 với thông tin mô phỏng. Đặc biệt:
**các trận là lịch game, không phải lịch EFL chính thức; Rust chưa sử dụng trực tiếp
54 thuộc tính, màn hình thử nghiệm hiện có cầu nối 54 → 20 chỉ số V3**.

---

# Football Match Lab 1.0

Ứng dụng Android Kotlin + Rust dành cho thử nghiệm mô phỏng trận đấu bằng dữ liệu PingFootball V3.
Giai đoạn 1 giữ nguyên công thức và chỉ số gốc, 20 đội Ngoại hạng Anh, TTS, bình luận và 48 âm thanh OGG.

- Phiên bản ứng dụng: `versionName 1.0-debug`, `versionCode 1`.
- GitHub Actions chạy kiểm tra dữ liệu và Rust, biên dịch thư viện Rust ARM64 tối ưu, rồi tạo APK Debug ký bằng khóa ổn định riêng của Football Match Lab.
- Không cần cấu hình GitHub Actions Secrets cho cách ký này. Khóa debug được lưu dạng Base64 tại `.github/signing/stable-debug.keystore.b64` để các bản debug sau có thể cập nhật đè.
- APK dùng mã gói `pl.mkrstudio.truefootball3.debug`, tách riêng với bản phát hành `pl.mkrstudio.truefootball3`.
- CI xác minh chữ ký và dấu vân tay, mã gói/phiên bản, cùng thư viện Rust bên trong APK. Tải APK và bằng chứng từ artifact của lần chạy GitHub Actions.
- Đây là bản Debug cài thử. Khóa đã lưu trong repository chỉ dùng cho bản Debug; không dùng để phát hành chính thức hoặc đưa lên Play Store.

Xem [MATCH_LAB_README.md](MATCH_LAB_README.md) để biết cách build và giới hạn kiểm thử.
