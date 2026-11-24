# DES calibration stage 3a: contact is not possession

Baseline: `31ab8b3a0ee930002f9caa49a8dbb817561ca68d`, [stage 2 evidence](https://github.com/tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio/actions/runs/45333737786).
Pass accuracy 70.6351%, 130.429 intercepted passes/match, 393.158 misplaced, 1833.701 attempts/match, 75.971 min ball in play.

Scope: resolve control at the actual defender contact using technical skill, balance, fatigue, incoming ball speed/height, reach and pressure. Successful control keeps the existing interception; an unsuccessful block emits a failed pass and a grounded loose-ball event. Schedule a positive-time, side-neutral race using the stage 2 motion profile. Preserve launch offside snapshot for accidental deflection and do not count the same pass twice. Record failed pass separately from actual possession loss. Persist pending recovery through checkpoint and cancel with existing live-play invalidation.

Limitations: this is stage 3a, not a full collision solver. A blocked ball drops in place; moving deflections, clearances and subsequent contested control, attacking first-touch errors, missed passes and goalkeeper-specific handling require later work. Possession time during loose recovery still uses the existing last-team convention. No tuning of pass accuracy or shot probability; no weakened regression gates. Three prior Rust tests and Android prefs compilation were failing at baseline. Re-run same population and targeted tests; inspect before accepting.
