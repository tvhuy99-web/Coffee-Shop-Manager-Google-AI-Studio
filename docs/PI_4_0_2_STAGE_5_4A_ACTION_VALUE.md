# PI 4.0.2 — Stage 5.4A: common on-ball action value

Baseline `596e02fc23acb92d6f9fcf22b005e5715d75e7b2`: [12/12 diagnostics jobs passed](https://github.com/tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio/actions/runs/45356048147).
1000-match baseline: pass accuracy 79.997%, 1693.43 attempted passes, 581.684 switches, 31.612 shots, 2.323 goals, 76.971 live minutes.

All actions now use U = p*benefit - (1-p)*loss - cost. These are dimensionless, fallible policy estimates, not calibrated xG/xT or the physical execution model. Position value includes progress, centrality and pressure relief; turnover cost depends on territory. Passing estimates consider technique, receiver touch, lane risk, pressure, distance and energy. Switch classification requires opposite wide channels, with no unconditional switch/width reward. Carry estimates the existing forward-only movement, including corridor and destination pressure; hold has no invented positional gain. Shot valuation considers distance, angle, skill and pressure. Group-correlated pass noise reduces the multiple-candidate lottery effect. Tactical risk, tempo, time wasting, transition context and offside awareness remain active.

No changes to execution, flight, control, timing configuration, movement integration or shot odds. No pass/shot count caps or target feedback. The coefficients are hypotheses to test, not empirical probabilities. Preserve same population cohorts, independent scenario tests and existing offside/timing/physics gates. Version and checkpoint policy identity change intentionally.

Next 5.4B: directional carries and shielding; no claiming those are already simulated. 5.5 off-ball support, 5.6 pressing. Review all regressions, not just Monte Carlo. Android and prior QA issues remain separate. The extraction parity gate remains a historical check, not a gate for intentional policy changes.
