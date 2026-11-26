# Hướng dẫn phát triển PingFootball

Đọc [ARCHITECTURE](docs/ARCHITECTURE.md), [DEVELOPMENT_RULES](docs/DEVELOPMENT_RULES.md), [ROADMAP](docs/ROADMAP.md) và [TESTING](docs/TESTING.md) trước khi thay đổi mã. Đây là bộ tài liệu đang có hiệu lực. Mã nguồn, hợp đồng dữ liệu và kiểm thử mô tả trạng thái hiện tại; roadmap mô tả mục tiêu chưa đạt. Nếu có mâu thuẫn, kiểm tra mã thật và báo rõ, không tự tuyên bố mục tiêu đã được thực hiện.

1. Rust là nguồn sự thật duy nhất của gameplay. Android không quyết định kết quả bóng đá.
2. Tìm crate sở hữu cơ chế trước khi sửa. Không thêm phụ thuộc ngược từ policy vào engine, không đưa công thức nghiệp vụ vào contracts.
3. Phân loại thay đổi: R (refactor), B (behavior), I (integration), T (test), D (docs). Không trộn đổi hành vi với dọn mã; R/D phải giữ gameplay exact parity.
4. Không nắn thống kê, tỷ số, quota hành động; sửa nguyên nhân vật lý/luật/nhận thức. Không dùng clamp, fallback hoặc test double để che lỗi.
5. Giữ thứ tự RNG, đơn vị, scheduler, schema, checkpoint, ABI; nếu cần thay đổi hợp đồng phải ghi rõ tác động và tăng định danh phù hợp.
6. Kiểm thử cô lập trước, sau đó chạy cổng tương ứng. Không xóa hoặc nới assertion, sửa golden chỉ để CI xanh; thiếu công cụ/bằng chứng phải báo lỗi.
7. Ghi rõ commit đối chứng, giả thuyết, phạm vi, kết quả thật và cách hoàn tác. Không báo PASS khi CI chưa hoàn tất.
8. Không tạo crate rỗng, mã giả hay tài liệu lịch sử dư thừa. Comment giải thích cơ chế, bất biến, tương thích hoặc lý do thiết kế hiện hành.
9. Phát triển từ B1 theo roadmap. Không nhảy sang chỉnh PI, chiến thuật hoặc UI để che lỗi vật lý/luật/condition.
10. Không tạo tag hay nhánh lưu trữ riêng chỉ vì sửa tài liệu. Giữ fixtures, lockfile, giấy phép và bằng chứng CI cần cho kiểm thử; không viết lại lịch sử Git.

Mỗi thay đổi phải kết thúc bằng: tệp/crate tác động; hành vi có đổi không; test và CI thực tế; giới hạn chưa xác minh. Không công bố release nếu L5 chưa đạt.
