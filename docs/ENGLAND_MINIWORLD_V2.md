# Giai đoạn 2: English football mini-world (bộ dữ liệu phát triển)

## Đã đặt dữ liệu trực tiếp trên `main`

Android assets: `app/src/main/assets/england_v2/`

- `clubs.json`: 232 câu lạc bộ có tên từ nguồn PingFootball 2025, giữ riêng `source_league_id` và `league_id` cho mùa giải thử nghiệm.
- `manifest.json`: kê 11 giải/nhóm và thống kê, thông tin nguồn, nhãn xác thực.
- `division_ENG_1.json` ... `division_ENG_7_4.json`: đội hình, hợp đồng, 54 thuộc tính, học viện, nhân viên và lịch lượt đi/lượt về, theo từng giải.
- `competition_rules.json`: khung FA Cup, EFL Cup, EFL Trophy, FA Trophy, FA Vase; chính sách đăng ký, kỳ chuyển nhượng, cấu trúc sự nghiệp.

| Nội dung | Số lượng |
|---|---:|
| CLB trong hệ thống giải Anh | 232 |
| Giải/nhóm (bao gồm 4 nhóm cấp 7 thử nghiệm) | 11 |
| Cầu thủ đội một | 5.640 |
| Trong đó có tên cầu thủ từ PingFootball v97 | 549 |
| Hồ sơ cầu thủ đội một được sinh thêm | 5.091 |
| Cầu thủ trẻ mô phỏng | 2.784 |
| Nhân viên mô phỏng (20 vai trò/CLB) | 4.640 |
| Trận giải giả lập mùa 2025/26 | 4.780 |
| Giải cúp được khai báo | 5 |

Mọi đội hình chính đều có ít nhất 24 cầu thủ và ít nhất 3 thủ môn. 54 thuộc tính chia thành:
13 kỹ thuật, 14 tinh thần, 8 thể chất, 9 thủ môn và 10 thuộc tính ẩn; thang điểm 1..20.

### Chất lượng dữ liệu và thông tin phải đọc trước

**Đây không phải database chính thức mùa 2026/27.** Tên đội và tên 549 cầu thủ
Premier League được chuyển từ bản PingFootball 2025. Các cầu thủ đội hạng dưới trong
GitHub hiện là hồ sơ **mô phỏng có đánh dấu**, không được nhận là roster ngoài đời.
Giá trị 54 thuộc tính trong bản seed GitHub cũng là giả lập, **chưa chuyển đúng tất cả
giá trị của bộ 54 thuộc tính V1 trước đây**.

Bản dữ liệu V1 gốc do người dùng cung cấp còn có 1.314 cầu thủ đã gắn với các
CLB thuộc hệ thống giải Anh và có chỉ số V1. Nguồn đó được lưu riêng để nhập nâng
chất lượng sau khi bộ khung chạy ổn định; chưa đưa trọn vào seed Android.

**Tất cả 4.780 trận được tự sắp xếp để thử game.** Đây không phải lịch thi đấu
thực tế EFL/FA. Lịch FA Cup, League Cup và các cúp khác chưa bịa cặp đấu; chỉ có
cấu trúc và điểm mở rộng để dựng vòng cúp khi người chơi bắt đầu sự nghiệp.

**Tất cả nhân viên** (huấn luyện viên, giám đốc, bác sĩ, tuyển trạch...) và tên
người của các hồ sơ được sinh là hư cấu. Không có dữ liệu người thật không có nguồn.
Stadium, board, tài chính và hợp đồng cho game cũng là giá trị mô phỏng, không được
dùng để nhận định tình hình tài chính thật của CLB.

**Nhóm `ENG_7` nguồn 2025 có 68 tên và không có phân bổ khu vực đủ tin cậy.**
Để thử mô hình mùa giải, chia thành 4 nhóm giả lập, mỗi nhóm 17 CLB. **Đây không
phải 4 giải cấp 7 chính thức, không tương đương phân bổ của FA.** Không dùng kết quả
đó cho lịch thật hoặc cơ chế thăng/xuống hạng cuối cùng.

Các CLB Wales đang chơi trong hệ thống Anh (như Swansea, Cardiff, Wrexham,
Newport) vẫn được giữ trong danh mục Anh, không chuyển quốc tịch thành ENG.

### Liên kết ứng dụng Android

- Xóa asset thử nghiệm cũ `match_lab/epl20_original.json` sau khi chuyển dữ liệu
  và mã tải sang `england_v2`.
- Màn hình thử nghiệm có bộ chọn giải đấu và CLB từ toàn bộ dữ liệu mới.
- `LabMatchInputFactory.kt` nạp bản ghi cầu thủ theo giải và tạo snapshot theo
  đội hình.
- **Lõi Rust hiện còn dùng 20 chỉ số V3**. Bản này chiếu 54 thuộc tính xuống 20
  trường khi mô phỏng trận thử nghiệm; đây là cầu nối chuyển tiếp, **chưa phải
  engine Rust 54 thuộc tính**. Không được khẳng định đã hoàn thành Phase 2.
- Cài đặt TTS và âm thanh cùng bộ bình luận được giữ nguyên; không có sửa đổi
  algorithm Rust trong bước thay database.

### Kiểm thử

```bash
python3 tools/verify_england_v2.py
bash tools/run_v2_parity.sh
```

Bộ `verify_england_v2.py` kiểm tra danh tính, 54 chỉ số, 3 thủ môn, 20 vai trò
nhân viên, 12 cầu thủ trẻ và tính duy nhất cặp đấu của lịch lượt đi/lượt về.
Các kiểm thử này không xác minh lịch thực tế hoặc chất lượng thuộc tính do AI tạo.

### Những phần giai đoạn 2 còn phải làm

1. Nhập nhất quán 1.314 cầu thủ của nguồn V1 (giữ `PLY-` ID) và thay bản ghi
   mô phỏng khi có dữ liệu xác thực; mở rộng danh sách CLB cấp 7 đúng phân nhóm.
2. Chuyển lõi trận Rust sang 54 thuộc tính thực sự, không thông qua cầu nối 20.
3. Thêm logic thăng/xuống hạng chính xác, play-offs, luật đăng ký và lịch cúp động.
4. Xây dựng màn hình quản lý nhân viên, hợp đồng, chuyển nhượng, học viện, tài chính,
   huấn luyện, lịch và kết quả cho chế độ sự nghiệp.
5. Kiểm thử Android/Android TalkBack, TTS và thống kê trận đấu.
6. Khi mini-world đã ổn định, thay bằng database hoàn chỉnh được xác minh.

Dữ liệu được tạo để **thử kiến trúc và luồng game**, không được tự công bố
là dữ liệu bóng đá ngoài đời đang cập nhật.
