# Kiểm soát ngữ cảnh bình luận PingFootball

## Vì sao phải sửa

Trong hai nhật ký Chelsea gặp Arsenal, tầng Kotlin đôi khi chọn câu đúng về mặt văn bản nhưng không phù hợp với sự kiện Rust:

- `PASS_FAILED` bị đọc thành pha rê bóng thất bại.
- `SECOND_BALL_WON` vẫn bị bình luận là bóng chưa có chủ.
- `TURNOVER` bị đồng nhất với một pha phản công nguy hiểm.
- `PASS_COMPLETED` có thể nói đến một bàn thắng chưa hề xảy ra.
- Thủ môn vừa bắt bóng nhưng lời bình mô tả kiểm soát bóng tại trung lộ.
- Câu khai cuộc hoặc câu về giai đoạn quyết định xuất hiện sai thời điểm.

Bản Java V3 cũ có `CommentaryVariableResolver` từ chối mẫu nếu thiếu danh tính hoặc dữ liệu bắt buộc và có `ConditionalTemplateSelector` giới hạn ngữ cảnh. Chúng ta áp dụng lại nguyên tắc đó trong Kotlin, không chuyển lại Match Engine sang Java.

## Các thay đổi trong nhánh này

- `LabCommentaryTemplatePolicy.kt`: kiểm tra đến từng template ID và thông tin metadata, thay vì chỉ chọn ngẫu nhiên trong một nhóm câu.
- `LabCommentaryEngine.kt`: ngừng suy diễn các hành động (tắc bóng, phản công, chuyền lỗi, thủ môn bắt bóng) từ nhãn sự kiện khác; chặn lời bình thời gian và tỷ số thiếu bằng chứng; không giả định tên CLB là tên cầu thủ.
- 15 câu trung tính bổ sung có cả `res/values` và `res/values-vi` để giữ khả năng TTS hai ngôn ngữ.
- `tools/run_commentary_regression.sh` và `tests/commentary/`: kiểm thử Kotlin/JVM với XML bình luận thực, 329 lần xác minh trên nhiều seed; không yêu cầu Android SDK.
- `.github/workflows/validate-port.yml`: job riêng dùng Gradle để chạy kiểm thử khi có commit hoặc Pull Request.

Không thay đổi: Rust Match Engine, dữ liệu đội/cầu thủ, 48 OGG, nội dung bốn file XML bình luận gốc và hai màn hình cài đặt TTS/âm thanh.

## Điều kiện bằng chứng hiện tại

Bộ lọc **không đánh dấu mọi câu là hợp lệ tuyệt đối**. Mẫu chỉ được phát khi thuộc nhóm sự kiện phù hợp và không đòi hỏi thông tin ngoài metadata. Một số câu có diễn đạt chủ quan vẫn thuộc bộ XML gốc và cần theo dõi qua log thực tế.

Các lựa chọn mang tính suy diễn (phản công cực nguy hiểm, phá pressing thành công, câu giờ, đánh chồng biên, dâng hàng thủ cao...) được hạn chế hoặc yêu cầu flag tương ứng. Những flag chưa được Rust xuất ra sẽ không làm phát những câu này.

Các tình huống mô tả tắc bóng và bóng hai sẽ ưu tiên câu trung tính, có sự kiện chắc chắn. Không suy diễn bàn thắng chỉ từ thời gian trận đấu.

## Kiểm tra

Trên máy có JDK 17+ và Kotlin CLI:

```bash
bash tools/run_commentary_regression.sh
```

Hoặc dùng Gradle (kể cả khi không có Android SDK):

```bash
bash gradlew -p tests/commentary run --no-daemon
```

Đã chạy thành công **329** kiểm tra Kotlin/JVM tại thời điểm tạo PR. Chưa biên dịch APK của nhánh mới, chưa xác minh TTS/TalkBack bằng trận đấu thực tế; cần thử lại trên Android sau khi merge.

**Nguyên tắc nghiệm thu:** lời bình phải phản ánh dữ kiện Rust, không được làm ngược lại; các tình huống thiếu bằng chứng chọn câu trung tính hoặc bỏ qua, không bịa thêm hành động.
