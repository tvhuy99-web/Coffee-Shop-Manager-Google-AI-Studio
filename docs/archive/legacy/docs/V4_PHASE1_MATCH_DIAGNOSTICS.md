# V4.1 - Chẩn đoán chuỗi tấn công (giai đoạn 1)

## Mục tiêu và phạm vi

Hệ thống chẩn đoán giúp giải thích **vì sao một trận đấu có ít cú sút/bàn thắng**, trước khi thay đổi xác suất hay thuật toán hành động. Đây là *observability*, không phải bản vá tăng số bàn thắng.

- Mã chính: `rust/football_match_engine/src/v4/diagnostics.rs`.
- Thu thập trực tiếp từ `MatchSessionV4::emit` trong cả `FullEvent` và `FastBatch`.
- Theo dõi vị trí **cầu thủ thực sự đang kiểm soát bóng** mỗi nhịp ở `MatchSessionV4::step_one_beat`.
- Không thay đổi RNG, mô hình chuyền/sút, diễn biến trận, event fingerprint, hoặc giao thức hiện tại.
- Tương thích ngược: kết quả V4 bổ sung trường JSON `diagnostics`, không sửa `summary` hay `events`. `eventSchemaVersion` vẫn là 4.

## Chỉ số theo đội, trong `matchResult.diagnostics.home/away`

| Trường JSON | Định nghĩa |
| --- | --- |
| `possessions` | Số chuỗi kiểm soát bóng bắt đầu, bao gồm bắt đầu từ một pha cố định. |
| `controlledBallSamples` | Số nhịp mô phỏng có quan sát được cầu thủ cầm bóng. Không phải số giây kiểm soát bóng. |
| `buildUpPossessions` | Chuỗi kiểm soát có ít nhất **2 đường chuyền hoàn thành**. |
| `finalThirdPossessions` | Số chuỗi có kiểm soát bóng ở 1/3 sân tấn công (từ mốc 70m theo hướng lên bóng). **Tối đa 1 lần/chuỗi**. |
| `penaltyAreaPossessions` | Số chuỗi kiểm soát bóng trong vùng cấm đối phương; **tối đa 1 lần/chuỗi**. |
| `penaltyAreaWithoutShot` | Chuỗi đã vào vòng cấm nhưng **không có cú sút**, proxy cho tấn công không kết thúc bằng sút. |
| `possessionsWithShot` | Chuỗi có ít nhất 1 cú sút, không phải tổng số cú sút. |
| `passesInShotPossessions` | Tổng đường chuyền thành công trong các chuỗi có sút; có thể chia cho `possessionsWithShot`. |
| `passesAttempted`, `passesCompleted` | Số lần chuyền, cùng quy ước với `summary` (việt vị là một lần chuyền thử). |
| `passesUnderHighPressure`, `failedPassesUnderHighPressure` | Các đường chuyền có `metadata.pressure >= 50`. Không phải tất cả sự kiện đều có metadata áp lực. |
| `passesByType` | Phân rã theo `passType`: attempted, completed, intercepted, misplaced, outOfPlay, offside. |
| `shots`, `shotsOnTarget`, `shotsOffTarget`, `shotsBlocked` | Phân rã cú sút, đối chiếu trực tiếp với `summary`. |
| `shotsFromBox` | Sút có `metadata.inBox = true`; không suy ra từ vị trí bóng ở cầu môn. |
| `shotXg` | Tổng xG thô từ metadata từng cú sút (có thể khác xG làm tròn ở `summary`). |
| `turnovers` | Một lần mất quyền kiểm soát cho mỗi nguyên nhân, tránh đếm trùng sự kiện phòng ngự đi kèm. |
| `clearancesEndingPossession` | Phá bóng chủ động kết thúc chuỗi, không mặc định là chuyền sai. |

### Giải thích nguyên nhân mất bóng

`turnovers` có các trường: `passIntercepted`, `passMisplaced`, `passOutOfPlay`, `firstTouchError`, `dribbleTackled`, `offside` và `otherUnclassified`.

Một pha chuyền bị cắt sinh ra cả `PassFailed` và `Interception`, nhưng **chỉ tăng `passIntercepted` một lần**. Lỗi đỡ bóng và bị đoạt bóng cũng vậy. `otherUnclassified` phản ánh trường hợp đội cầm bóng đổi mà không có sự kiện mất bóng được phân loại; số này cao là dấu hiệu cần bổ sung instrumentation.

### Giới hạn cần biết

1. "Vào 1/3 cuối sân" và "vào vòng cấm" ở đây là **số chuỗi sở hữu bóng đạt khu vực**, không phải lượt xâm nhập vật lý hay số cơ hội rõ rệt.
2. Một đường chuyền được ghi hoàn thành nhưng cầu thủ lập tức đỡ hỏng **không** được tính là đã kiểm soát bóng ở điểm đến. Phép đo dựa trên `FirstTouch` sạch hoặc nhịp sau đang kiểm soát bóng.
3. Kết quả có thể chịu ảnh hưởng bởi số nhịp mô phỏng mỗi trận, nên `controlledBallSamples` không tương đương với thời gian giữ bóng.
4. Không dùng chỉ số của một trận đơn lẻ để tuyên bố mô phỏng chân thực. Các fixture hiện tại **tổng hợp**, với thuộc tính chủ nhà 14 và khách 12, không phải squad thật.

## Chạy kiểm thử / xuất báo cáo

```bash
cd rust/football_match_engine
cargo test --all-targets

# Xuất 12 trận cố định: mỗi đội có funnel, nguyên nhân mất bóng và pass-type.
mkdir -p ../../build-evidence
V4_DIAGNOSTICS_REPORT_PATH=../../build-evidence/v4_attacking_diagnostics.json \
  cargo test --lib v4::diagnostics::tests::export_attacking_diagnostics_sample -- --ignored --nocapture

# 128 trận tổng hợp: chỉ số trung bình ở metrics.avgPossessionsPerMatch,
# metrics.avgPenaltyAreaWithoutShotPerMatch, metrics.avgFirstTouchErrorsPerMatch, ...
V4_REALISM_MATCHES=128 V4_REALISM_REPORT_PATH=../../build-evidence/v4_realism_baseline.json \
  cargo test --lib v4::attribute_impact_tests::monte_carlo_realism_baseline_report -- --ignored --nocapture
```

Workflow `.github/workflows/validate-v4-p2.yml` chạy cả ba nguồn bằng chứng (attribute-impact, realism baseline, attack diagnostics) và tải lên artifact `v4-impact-realism-and-attack-diagnostics`.

## Cách đọc để quyết định bước V4.2

- **`finalThirdPossessions / possessions` thấp**, và `passIntercepted` hoặc `passMisplaced` cao: kiểm tra mạng chuyền, áp lực và khoảng cách hỗ trợ.
- **`penaltyAreaPossessions / finalThirdPossessions` thấp**: kiểm tra di chuyển không bóng, vượt tuyến và phối hợp ở biên.
- **`penaltyAreaWithoutShot / penaltyAreaPossessions` cao**: kiểm tra lựa chọn hành động, khả năng tạo cú sút trong vòng cấm.
- **Có đủ cú sút nhưng ít bàn thắng**: chuyển sang V4.3, so sánh `shotXg` với ghi bàn và hiệu quả thủ môn.
- **`otherUnclassified` cao**: mở rộng phân loại sự kiện trước khi dùng chỉ số để ra quyết định.

Các tỷ lệ này **chưa có ngưỡng đạt/trượt** cho đến khi so sánh với dữ liệu kiểm chuẩn và nhiều mẫu fixture. Giai đoạn 1 đo đạc và kiểm chứng dữ liệu; giai đoạn 2 mới hiệu chỉnh hành vi.
