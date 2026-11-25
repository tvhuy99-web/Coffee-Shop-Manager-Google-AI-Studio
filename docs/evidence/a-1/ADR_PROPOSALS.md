# A-1 — ADR decision record (proposed, awaiting owner approval)

Source: Revision 3, section 14.3. These are proposed decisions and actual implementation facts, not an approval attributed to the owner.

| ADR | Recommended decision for approval | Verified implementation / remaining action |
|---|---|---|
| 001 | Choose PI 4.0.28 c2c392d8 as baseline, retain 4.0.27, no revert. | Both baseline refs and tags point to exact commits. 904 audit simulations and exact parity on 4.0.28 PASS. |
| 002 | Work on integration branch and accept main changes only after reviewed L0–L4 green PR. | Main unchanged, separate draft PR #6/#7/#8. No direct push to main. Actual ruleset enforcement is not configured; owner approval and protection settings remain open. |
| 003 | Preserve original ZIP bytes and SHA-256 manifests permanently on evidence/a1-historical; retain CI artifact copies 90 days. Keep index and source documents in docs/archive before deleting legacy in A1. | 55 archives verified and committed; six complete event traces pending durable archive. Large ZIP split into lossless <=64 MiB parts with original hash. The evidence branch differs from docs/archive location recommended by the specification; approval needed for this storage arrangement. |
| 008 | Require bit-exact within pinned Rust/Cargo.lock and identical target; verify ARM64/Linux compatibility separately before release, without assuming cross-target equality. | Linux x86_64 locked replay passes; ARM64 parity not measured. Owner must decide whether bit-exact cross-target parity is mandatory before release. |

## Remaining
- Confirm ADR-001/002/003/008 explicitly; the general request to continue is not represented as a signed acceptance decision.
- Repository rulesets endpoint returned an empty list when checked. Tags are named refs, not inherently immutable; owner should protect baseline tags against update/deletion and main against direct pushes using GitHub rulesets. Do not modify owner security settings implicitly.
- ADR-004–007 and 009–011 remain open at their relevant stage; no policy change or gameplay version bump.
- Do not close A-1 until full event trace archive and evidence completeness are verified. Preserve production behavior and do not merge A0 before A-1 acceptance.
