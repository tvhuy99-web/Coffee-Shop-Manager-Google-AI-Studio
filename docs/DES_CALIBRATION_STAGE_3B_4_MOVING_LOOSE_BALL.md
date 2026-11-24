# DES stage 3B–4: loose-ball motion and pass errors

Baseline: `879276644ecb50af082a3e3b25e887a0447ba56c`, [stage 3A](https://github.com/tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio/actions/runs/45336547728).
Pass accuracy 70.6271%, 1824.45 attempts, 391.191 misplaced turnovers, 106.253 blocked loose passes, 8.263 attacking recoveries per match.

Preserve dissipated momentum after a block. Missed passes remain at their physical endpoint; both teams can recover them. Let an inaccurate kick count as a completed pass if the intended receiver actually contacts it. Heavy touches also lead to a scheduled moving recovery. Preserve the Law 11 launch snapshot (renew it after a legal first touch), checkpoint recovery motion and cause, separate execution errors from unreachable receivers and count actual turnovers only when possession changes. Handle boundary and goal-line crossings during loose recovery. No edits to base pass accuracy or shooting probabilities.

Limitations: simple linear drag, not full 3D foot collision/spin; routine control under 7 m/s is an approximation; repeated fast contacts, planned clearances, goalkeeper-specific handling, all-teammate in-flight reception and original pass boundary cases remain. Only the chosen runner's predicted path is replayed. A missed intended pass recovered later still counts as failed. This is a test candidate, not a release. Compare identical populations and targeted tests; keep existing QA gates. QA-10 and Android prefs failure were present at baseline.
