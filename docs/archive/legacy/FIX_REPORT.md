# Báo cáo sửa lỗi: PureRustKotlin Clean → Fixed

**Phạm vi:** Sửa các lỗi còn tồn đọng sau khi tinh gọn 4.771 file xuống 149 file.
Không sửa thuật toán mô phỏng, dữ liệu EPL20, mẫu bình luận, 48 âm thanh OGG.

## Lỗi đã xử lý

1. **Lỗi cú pháp Kotlin:** thay chuỗi chứa xuống dòng thật trong `LabMainActivity.kt`
   bằng ký tự escape `"\\n"` khi ghi nhật ký kiểm chuẩn.
2. **Lỗi Gradle:** bỏ phụ thuộc vào file `CommentaryVariable.java` đã xóa.
   Task `validateCommentaryTokens` lấy danh sách 27 biến từ `LabCommentaryEngine.kt`
   và đối chiếu XML tiếng Việt/tiếng Anh. Vẫn chạy trước `preBuild`.
3. **Script đối chứng hỏng:** `run_v2_parity.sh` không còn gọi
   `run_match_engine_behavior_tests.sh` không tồn tại. Script kiểm tra tính toàn vẹn
   các file kết quả Java V3 đã lưu rồi chạy `cargo test --all-targets` nếu có Cargo.
4. **Thiếu công cụ build Rust Android:** thêm `tools/build_lab_rust_android.sh`
   sử dụng `cargo-ndk` để tạo thư viện JNI `.so`.
5. **Tài liệu/CI cũ:** cập nhật README, nguyên tắc kiến trúc, GitHub Actions
   theo cấu trúc không có mã Java trong Android.
6. **Kiểm tra tĩnh:** mở rộng script `verify_lab_v2.py` để kiểm tra lỗi này trong
   các bản sau (đường dẫn chết, đồng bộ biến bình luận, nội dung assets...).

## Kiểm thử đã thực sự thực hiện

- `python3 tools/verify_lab_v2.py`: **95/95 PASS**.
- `python3 tools/verify_parity_fixtures.py`: **PASS**, xác nhận SHA-256
  của 7.044 sự kiện mẫu Java V3 và số sự kiện khớp với ba trận tham chiếu.
- `bash -n tools/run_v2_parity.sh tools/build_lab_rust_android.sh`: **PASS**.
- `python3 -m py_compile` cho hai script Python: **PASS**.
- `REQUIRE_RUST=1 bash tools/run_v2_parity.sh`: **báo thiếu Cargo, exit code 2**,
  tức không ngộ nhận kết quả tĩnh là parity Rust.
- `bash tools/build_lab_rust_android.sh`: **báo thiếu Cargo, exit code 2**;
  không tạo `.so` giả.
- Dùng trình biên dịch Kotlin để rà cú pháp: không thấy thông báo cú pháp;
  biên dịch hoàn chỉnh vẫn lỗi thiếu thư viện Android (SDK không có trong môi trường).

## Chưa được nghiệm thu

- Chưa chạy `cargo test` vì môi trường này thiếu Rust/Cargo.
- Chưa chạy Gradle/Android SDK, chưa tạo `.so` hoặc APK.
- Chưa kiểm tra TalkBack, TTS, âm thanh và ứng dụng trên thiết bị Android.
- Chưa xác minh Rust tương đương 100% Java V3 qua thực thi.

### Lệnh cần chạy trên máy Android Studio + Rust/NDK

```bash
REQUIRE_RUST=1 bash tools/run_v2_parity.sh
bash tools/build_lab_rust_android.sh
bash gradlew :app:validateCommentaryTokens
bash gradlew :app:assembleDebug
```

**Kết luận:** Các lỗi đường dẫn/cấu hình/cú pháp đã biết được xử lý trong mã nguồn;
đây chưa phải chứng nhận APK chạy thành công hay Match Engine đã đạt parity.
