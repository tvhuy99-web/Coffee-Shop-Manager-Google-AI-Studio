# A8 — Nghiệm thu theo quyết định chủ dự án

Ngày: 2026-11-26.
Phạm vi: Gói 1–4 (A-1–A8), không triển khai B1–B6.
Ứng viên tham chiếu: e540c51a675dce30f878b01a211262740608274b; CI 45747583976 và tài liệu bàn giao d52263227f0ae93ef87de6c99c87c4ff0f666816, CI 45748853837 đều 19/19 PASS.

## Xác nhận của chủ dự án
Chủ dự án báo đã thử trên điện thoại, trải nghiệm khá ổn và yêu cầu xác nhận giai đoạn A8 thành công. Ghi nhận A8 **ACCEPTED_BY_OWNER** theo ngoại lệ nghiệm thu thực tế và quyết định dừng phạm vi phát triển. Không gán kết quả PASS cho từng ca chưa được báo cáo. Đây không phải bằng chứng L5 đầy đủ hoặc chứng nhận release.

## Dữ liệu người dùng cung cấp (nguyên văn)
> V4.0 FastBatch: 100/100 trận xong trong 20613 ms • Seed-Replay khớp: 100%
> Động cơ: Rust Engine 4.0 (20 trận trong 10304ms)
> AFC Bournemouth (4-3-3/Cân bằng)
> vs Chelsea FC (4-3-3/Cân bằng)
>
> Thắng / Hòa / Thua: 10 – 4 – 6
> Bàn trung bình/trận: 2,80 – 2,60
> xG trung bình/trận: 7,00 – 6,12
> Sút trung bình/trận: 32,5 – 29,9

Derived: FastBatch 206.13 ms/match (4.85 matches/s); second run 515.2 ms/match (1.94 matches/s). Combined goals 5.40, xG 13.12, shots 62.4 per match. These are user-reported aggregate observations, not a controlled comparative realism study. The second run's mode is unspecified; do not label it FullEvent. The installed APK hash, phone model, Android version, voice, seed/config and raw traces were not provided. Reference candidate is not an independently verified identity of the installed APK.

## Final disposition
- Owner acceptance: ACCEPTED.
- Automated L0–L4: PASS on the frozen candidate.
- Formal L5: NOT_FULLY_VERIFIED; owner accepts closing A8 without the complete instrumented device matrix. No invented measurements or scenario PASS.
- ADR-009 numeric thresholds, device-specific RAM/thermal/drift/AudioFocus/force-stop evidence and exact cross-target parity remain unverified.
- Retain targetSdk 29 and PI 4.0.28. High shot/xG/goal totals are retained as a baseline observation; no gameplay tuning, no B1–B6.
- No production release or main merge is implied by this acceptance. Previously recorded architectural debt remains listed; A8 approval does not establish every outstanding A3–A6 criterion.
