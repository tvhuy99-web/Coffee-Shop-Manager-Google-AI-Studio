# Cập nhật giai đoạn 2: dữ liệu bóng đá Anh

Đã đưa **English mini-world** lên `main`: 232 CLB, 11 giải/nhóm, 54 thuộc tính
mô phỏng, 5.640 cầu thủ đội một, 2.784 cầu thủ trẻ, 4.640 nhân viên và
4.780 lịch trận **giả lập**. File thử nghiệm cũ `epl20_original.json` đã bị xóa.

Đọc [hướng dẫn dữ liệu Anh V2](docs/ENGLAND_MINIWORLD_V2.md) để phân biệt
thông tin lấy từ bản nguồn PingFootball 2025 với thông tin mô phỏng. Đặc biệt:
**các trận là lịch game, không phải lịch EFL chính thức; Rust chưa sử dụng trực tiếp
54 thuộc tính, màn hình thử nghiệm hiện có cầu nối 54 → 20 chỉ số V3**.

---

# Football Match Lab V2, Pure Kotlin + Rust (bản sửa lỗi cấu hình và kiểm thử)

## Mục tiêu

Đây là **ứng dụng thử nghiệm Match Engine**, không phải game quản lý CLB hoàn chỉnh.
Trên Android, Kotlin phụ trách UI, bình luận, TTS, âm thanh; Rust phụ trách mô phỏng.
Không dùng bộ 54 chỉ số mới ở giai đoạn 1. Dữ liệu thử nghiệm giữ 20 đội Ngoại hạng Anh
và 549 hồ sơ cầu thủ PingFootball nguyên bản.

**Không có mã Java trong nguồn ứng dụng Android.** Các file kết quả Java V3 trong
`tests/parity/` chỉ là dữ liệu đối chứng đã lưu, không phải mã nguồn Java đang chạy.
Năm file `.java` còn trong `tests/parity/` là các bộ sinh fixture lịch sử, hiện
**không thể chạy riêng** do lớp Java V3 gốc không được đóng gói trong ZIP tinh gọn.

## Màn hình

- `Bắt đầu` và `Tải lại`: nút giữ chỗ.
- `Cài đặt`: **hai mục riêng** `Cài đặt TTS` và `Cài đặt âm thanh`.
- `Thử nghiệm`: chọn đội chủ nhà/khách từ 20 CLB và mô phỏng bằng Rust nếu thư viện JNI có mặt.
- `Nhật ký`: công cụ hỗ trợ xem lỗi và kết quả kiểm chuẩn.

Giữ 48 âm thanh `.ogg`, các XML câu bình luận tiếng Việt/Anh và cấu trúc điều hướng
bình luận PingFootball. Đọc `app/src/main/assets/AUDIO_LICENSES.md` trước khi phân phối.

## Cấu trúc nguồn

- `app/`: Kotlin Android, giao diện, TTS, âm thanh, bình luận, asset EPL20.
- `rust/football_match_engine/`: mã nguồn Rust mô phỏng, JNI và kiểm thử parity.
- `tests/parity/`: kết quả kiểm chuẩn Java V3 (3 trận, 7.044 sự kiện) đã được ghi sẵn.
- `tools/`: kiểm tra tĩnh, kiểm tra fixture, chạy test Rust, build Android `.so`.

Không kèm `competition_data_current` và `competition_data_2025`: ứng dụng chỉ thử trận
đấu riêng lẻ, không tạo mùa giải. Không cần khôi phục hai thư mục này.

## Kiểm thử khả dụng trên máy phát triển

Từ thư mục gốc dự án:

```bash
python3 tools/verify_lab_v2.py
python3 tools/verify_parity_fixtures.py
bash tools/run_v2_parity.sh
```

`run_v2_parity.sh` **không còn gọi** script đã xóa `run_match_engine_behavior_tests.sh`.
Nó kiểm tra toàn vẹn nguồn, các fixture Java V3 đã lưu, rồi gọi `cargo test --all-targets`
**nếu có Cargo**. Khi không có Cargo, script thông báo rõ chỉ đạt kiểm tra một phần.
Muốn bắt buộc phải có Rust và nhận exit code lỗi khi thiếu Cargo:

```bash
REQUIRE_RUST=1 bash tools/run_v2_parity.sh
```

**Kiểm tra fixture không phải là chạy Java V3.** Chỉ khi `cargo test` thực sự vượt qua
các bài Rust so sánh với fixture, mới có cơ sở nghiệm thu tương đương cho những ca đó.

## Biên dịch Rust cho Android

Yêu cầu: Rust stable, Android SDK/NDK, `cargo-ndk`, target Android cần thiết:

```bash
cargo install cargo-ndk
rustup target add aarch64-linux-android
bash tools/build_lab_rust_android.sh
```

Script mặc định tạo `app/src/main/jniLibs/arm64-v8a/libfootball_match_engine.so`.
Có thể chọn nhiều ABI bằng biến `RUST_ANDROID_ABIS`, ví dụ:

```bash
RUST_ANDROID_ABIS=arm64-v8a,x86_64 bash tools/build_lab_rust_android.sh
```

Để kiểm tra Gradle và build Android, cần Android SDK 35, Build Tools 35.0.1 và JDK 17:

```bash
bash gradlew :app:validateCommentaryTokens
bash gradlew :app:assembleDebug
```

`validateCommentaryTokens` giờ đọc chính các biến `mapOf` trong **LabCommentaryEngine.kt**,
không còn phụ thuộc Java đã bị loại. Nó đối chiếu token giữa 895 câu mỗi ngôn ngữ
và được gắn vào `preBuild`.

Không sử dụng fallback Java: khi thiếu Rust `.so`, màn hình phải báo rõ lỗi thư viện,
không tự nhận trận đấu đã chạy bằng Rust.

## Mức độ đã xác minh và giới hạn

**Đã kiểm tra trong đợt sửa:** cấu trúc nguồn, dữ liệu EPL20, checksum 48 file OGG,
895 mẫu bình luận cho mỗi ngôn ngữ, đối chiếu token Kotlin/XML, ba fixture trận
đấu và SHA-256 của 7.044 sự kiện Java lưu sẵn; script kiểm tra không còn đường dẫn chết.

**Chưa xác minh trên máy soạn thảo này:** biên dịch `cargo test`, tạo `.so`,
`assembleDebug`, chạy JNI trên thiết bị Android, điều hướng TalkBack/TTS và phát âm thanh
thực tế. Không kèm APK hay `.so` đã biên dịch. Tên 'Pure' nghĩa là mã nguồn Android
không còn Java, **không** có nghĩa đã được chứng nhận hoạt động hoàn chỉnh.

Bản sửa này tập trung vào lỗi chặn build/kiểm thử, không thay đổi công thức mô phỏng,
dữ liệu cầu thủ, các câu bình luận hoặc nội dung âm thanh.


## APK Debug ký ổn định

- GitHub Actions chạy khi push lên `main` hoặc chọn **Actions → Build Football Match Lab stable debug APK → Run workflow**.
- Workflow kiểm tra dữ liệu và fixture, chạy `cargo test --all-targets`, biên dịch Rust ARM64 ở chế độ tối ưu, build APK Debug, rồi kiểm tra APK, chữ ký, mã gói, phiên bản và thư viện native.
- Khóa debug riêng của Football Match Lab được lưu Base64 trong `.github/signing/stable-debug.keystore.b64`. Khóa và mật khẩu debug được cố ý để trong repository theo cách của repo mẫu; vì vậy đây không phải bí mật và tuyệt đối không được dùng để ký bản phát hành chính thức.
- Không cần tạo GitHub Actions Secrets cho quy trình Debug này. Các lần build sau dùng cùng khóa, nên APK mới có thể cập nhật trực tiếp APK Debug trước đó.
- APK có `applicationId = pl.mkrstudio.truefootball3.debug`, `versionName = 1.0-debug`, `versionCode = 1`. Nó cài cạnh bản chính thức `pl.mkrstudio.truefootball3` như một ứng dụng riêng, với dữ liệu riêng.
- APK và bằng chứng build được tải xuống từ artifact của GitHub Actions (lưu 30 ngày). Bản build này phù hợp để cài thử; không dùng để phát hành trên Google Play hoặc làm phép đo hiệu năng production.
- Nếu sau này cần phát hành production, cần một khóa release riêng tư và phải tăng `versionCode` mỗi lần cập nhật. Không dùng khóa debug đã commit cho mục đích đó.
