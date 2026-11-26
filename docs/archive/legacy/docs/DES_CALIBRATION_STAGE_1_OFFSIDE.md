# DES calibration stage 1: offside awareness

Baseline: `541eee8cb69efb3a025fdd48885a7f40dbc0655f`, 1000 synthetic 14-vs-12 matches, [CI](https://github.com/tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio/actions/runs/45323523334).
Pass accuracy 50.5901%, 1392.689 attempted passes/match; diagnostic breakdown approximately 95.16 offsides/match, 349.63 interceptions, 230.95 misplaced passes.

Hypothesis: the existing tactical movement rewards forward runs without a receiving-line reference, and pass evaluation rewards an already-offside teammate. Correct both at decision level, preserve authoritative Law 11 position and involvement checks, keep errors possible, and permit runs after the ball is released. No change to pass execution probability, defender reach, shooting, ball-flight timing or added time. This is a calibration candidate, not a release.

Evidence: real decision/movement scenario tests and repeat the identical six population cohorts. Inspect per-type pass outcomes, offside frequency, attacking productivity, failed seeds and regressions. No population realism threshold is a hard CI gate. Use the commit SHA as the experiment identifier; before release reconcile engine version/replay compatibility.
