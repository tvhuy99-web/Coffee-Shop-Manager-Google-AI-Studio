# PingFootball 4.0 Foundation

Nền tảng mô phỏng bóng đá bằng Rust Discrete Event Simulation (DES), với Android đảm nhiệm trình diễn, lời bình và âm thanh. Phát triển tiếp từ B1: Physics, Rules và Condition.

## Tài liệu chính thức

- [AGENTS.md](AGENTS.md): quy tắc làm việc với mã nguồn.
- [Kiến trúc](docs/ARCHITECTURE.md): cấu trúc đang tồn tại và ranh giới trách nhiệm.
- [Hiến chương kỹ thuật](docs/DEVELOPMENT_RULES.md): hợp đồng bất biến và quản trị thay đổi.
- [Lộ trình](docs/ROADMAP.md): B1–B6, mục tiêu và điều kiện nghiệm thu.
- [Kiểm thử](docs/TESTING.md): lệnh build, CI, parity, audit và nghiệm thu thiết bị.

## Build nhanh

```sh
cd rust
cargo test --workspace --locked --all-targets
cd ..
bash tools/build_lab_rust_android.sh
bash gradlew :app:validateCommentaryTokens :app:assembleDebug --no-daemon
```

Rust và dependency được khóa bằng manifest toolchain và Cargo.lock. APK Debug ARM64 không phải bản phát hành production.

Dữ liệu `app/src/main/assets/england_v2/` là bộ dữ liệu phát triển, có hồ sơ, thuộc tính, lịch và tài chính mô phỏng; không phải database bóng đá hiện thực đã xác minh. Giữ provenance trong asset manifest và [giấy phép âm thanh](app/src/main/assets/AUDIO_LICENSES.md).

Trạng thái CI xem tại [GitHub Actions](../../actions). Chỉ kết luận PASS dựa trên lượt chạy tương ứng với commit cần nghiệm thu.
