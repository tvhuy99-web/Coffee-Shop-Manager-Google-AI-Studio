# Revision 3 — Đối chiếu cuối Phần A (Gói 1–4)
Ngày 2026-11-26. Nguồn chuẩn: docs/FOOTBALL_SIMULATION_4_SPEC.md, Mục 6.5, 11, 12, 14.3. So sánh mốc tích hợp d7980f215e10096fe59b87efeb3c57fdb1df8aae (CI 45750744347: 19/19 PASS) với ứng viên kiểm tra bổ sung 95f1f22. Không tự nâng PI, không triển khai B1–B6.

## Mục 12: từng chặng
| Mục | Kết quả / bằng chứng | Phần còn lại, quyết định |
|---|---|---|
| A-1.1 | Ref/tag PI 4.0.27 và 4.0.28 tồn tại, baseline c2c392d8 được sử dụng | ADR-001, protection rule/tag chưa được chủ dự án duyệt riêng; không tự sửa bảo mật |
| A-1.2 | Rust 1.100.0, Cargo.lock, NDK/JDK/Gradle/SDK ghi nhận; lock graph gate | Đã thực hiện |
| A-1.3–5 | 61 archive gốc tại d070818e, hash manifest, raw population/audit 904, 1.396 golden, 7 marker, inventory | Vị trí archive khác docs/archive; ADR-003 phê duyệt chính thức còn mở |
| A0.1–3 | Version/manifest negative case, hành vi DES JVM thay beat cũ, grouped substitution/immutable replay, thiếu tool fail | Đã thực hiện |
| A0.4–5 | L0–L4 hợp nhất, locked, 19/19 PASS; 1.396 golden và audit 54, APK | Strict rustfmt còn warning, không gọi PASS |
| A1 Rust/Android | 34 Rust legacy, 9 JNI legacy bị loại; V3 preference migration và tài liệu/archive | Production V4 duy nhất; compatibility label/shim cũ không phải engine thứ hai |
| A1 Tools/docs | Static gate cập nhật, archive, README về spec, dữ liệu âm thanh giữ | Đã thực hiện; archive lịch sử giữ nguyên |
| A2 | Leaf contracts + units/ports, VersionManifest4, checkpoint schema v2, negative tests | DTO input/events/version còn một số coupling Engine; không giả định leaf toàn bộ |
| A3.1 | DecisionObservation/SpatialForecastPort/RngSampleSource, pure PI/Coach crate không import Engine state, adapter read-only; thêm negative test thay attr đối thủ không đi vào observation | Còn compatibility context (opponent line height, exact phase, own attrs/psychology, roster index, exact spatial forecast), ghi ADR ngoại lệ; không tuyên bố hết toàn tri |
| A3.2 | Substitution ở Engine; coach proposal adapter | Scheduling/apply sở hữu Engine, giữ thứ tự legacy |
| A3.3 | perception.rs còn compatibility re-export, không chứa policy riêng | Giữ shim để không phá public API; xóa chỉ khi kiểm chứng tất cả consumer và chấp nhận breaking API |
| A4 | Session 3730→1289 dòng + 6 nhóm transition, atomic API/DES, rollback, counter/clock/identity guards | World state vẫn authoritative duy nhất; payload checkpoint v2 còn flat vì exact compatibility. Không đổi wire schema. Strict INV-01/02 ở mọi bước và full facts/delta ownership chưa đạt; không thay gameplay để cưỡng ép |
| A5 | 10 crate production, dependency DAG, locked all targets, fixture ở rust/tests/engine, source wiring gate không hardcode include_str; thêm Cargo external test target | Engine physical directory còn tên legacy; calibration public API giữ để tránh break API, có integration test riêng. Không đổi package identity hoặc đường JNI. Strict fmt chưa đạt |
| A6.1–4 | 16 JNI exports (15 V4/DES + manifest), ARM64 signed debug, controller/projection/commentary/audio, package namespace chuẩn hóa, Gradle/JVM PASS; giữ targetSdk29 | Activity còn UI/dialog/lifecycle orchestration, không phải simulation owner; final APK sau namespace chưa được chủ dự án thử độc lập |
| A7 | 19/19 PASS tại d7980f2; 1.396 golden, 12 FullEvent, 1.384 cross-mode, 7 marker, command, audit 54, benchmark | Ứng viên 95f1f22 và báo cáo divergence cập nhật phải chạy CI mới. Không thay golden |
| A8 | ACCEPTED_BY_OWNER, 100/100 FastBatch 20613ms, Seed-Replay 100%; hồ sơ OWNER_ACCEPTANCE_VI.md | Formal L5 NOT_FULLY_VERIFIED; không yêu cầu người dùng làm thêm theo quyết định nghiệm thu; không release |

## Mục 6.5: phạm vi kiểm chứng invariant
| ID | Cổng hiện có | Giới hạn |
|---|---|---|
| INV-01 | Owner actor/side/eligibility guard, loose/in-flight no-owner guard, read-only verifier | Controlled ở mọi restart/period boundary chưa được gate tuyệt đối; không sửa legacy ownership ở Phần A |
| INV-02 | Finite state, realized movement budget, negative teleport/inactive displacement | Full trajectory/acceleration và không clamp boundary cần thay đổi vật lý; không làm B1 |
| INV-03 | Scheduler generation/order, clock monotonic, event/command journal và transaction rollback | PASS phạm vi gate hiện tại |
| INV-04 | Action actor, roster identity, grouped substitution/limits | PASS phạm vi test; không gán mọi nhánh luật là chứng minh hình thức |
| INV-05 | GoalDeltaAudit, own-goal/restart/shootout regression | Không chứng minh đầy đủ crossing facts cho mọi goal |
| INV-06 | Deadball guards, period-end/injury regression | PASS phạm vi test |
| INV-07 | Motion interval, fatigue range, immutable profile guard | Không điều chỉnh cách tính fatigue deadball legacy |
| INV-08 | Flight delta references, atomic API/DES rollback, invalid snapshot rejection; rating delta mới | Không có facts/delta riêng toàn bộ subsystem |
| INV-09 | Golden markers/restore, RNG/queue/intent/ledger, public integration tests | PASS tại mốc xanh; gate mới đang chờ |
| INV-10 | Clock/possession accounting, cross-mode diagnostics, calibration observational test | Không chứng minh mọi action-counter path bằng model checking |
| INV-11 | Sequence/time monotonic, presentation session generation, stale save/delete tests | Không đổi event wire schema để thêm ID riêng; envelope matchId giữ nguyên |
| INV-12 | Fulltime one-close, reject post-match commands, preserve checkpoint on failed JNI finish | PASS phạm vi test |

## ADR và điều kiện bàn giao
- ADR-001–008, 011: ghi quyết định kỹ thuật đang sử dụng và ngoại lệ, không mạo nhận phê duyệt chủ dự án. ADR-006 speculative rollback, ADR-010 realism: nằm ngoài phạm vi đã chốt. ADR-009 owner accepted A8, ngưỡng L5 chưa đo.
- Không đổi rulesets bảo mật, không push main, không release production. Nợ strict fmt không tự động biến thành PASS; nên tách format-only commit và sửa static substring gates nếu được yêu cầu.
- B1–B6 bị loại khỏi phạm vi; không chuyển nợ sang một giai đoạn sẽ tự động triển khai. Mọi mục đòi đổi gameplay giữ nguyên như baseline, không che lỗi bằng bỏ test hoặc nới epsilon.
- Hồ sơ này là phân loại thực tế theo spec, không phải tuyên bố tất cả tiêu chí đạt. Các bổ sung mới chỉ được hợp nhất sau L0–L4 xanh.
