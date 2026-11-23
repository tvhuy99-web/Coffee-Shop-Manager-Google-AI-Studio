# V4 Attribute Impact and Realism Testing Protocol

## Purpose and evidence levels

The goal is to demonstrate real football behavior changes, not simply count attribute names in Rust.

1. SCHEMA: All 54 attributes deserialize, validate, enter the input fingerprint; 50 in-match attributes have runtime reads and four hidden career attributes have dedicated inter-match roles.
2. ISOLATED MECHANISM: Change exactly one named attribute while holding situation and PRNG seed constant; assert the intended directional result.
3. MATCH-LEVEL CAUSAL PROBE: Keep seed, roster IDs, tactics and environment constant; change one named attribute across the relevant player cohort (10 outfield players or one goalkeeper). Compare actual event timeline and match summary, not the input fingerprint.
4. REALISM: Compare population statistics using a transparent fixture/sample design and a separate holdout seed set. Do not calibrate from random synthetic squads alone.

Passing any lower level does not prove a higher one.

## Currently strict, direction-sensitive tests

- Command_of_area increases interventions on high aerial deliveries.
- Handling increases clean aerial claims.
- Punching increases aerial punches rather than catches.
- Kicking increases long distributions with throwing held constant.
- Stamina reduces aerobic and anaerobic depletion from repeated sprints.
- Big_matches activates on high-importance fixtures, not ordinary fixtures.
- Tackling, marking and strength: controlled attacker/defender duels produce directional tackle-win changes.
- Goalkeeper one_on_ones: controlled 1v1 shot-stopping reduces conceded goals.
- Passing, first_touch, finishing and dribbling each change a true Rust event timeline in at least one of 20 paired-seed matches.
- Existing V4 stage tests also gate passing completion, first-touch mistakes, goalkeeper reflexes, handling, finishing, consistency and natural fitness.

## Extended 50-attribute matrix

The CI runs 12 paired seeds for all 50 in-match attributes: 1,200 complete 90-minute match simulations (two outcomes per seed). Each JSON record contains attribute name, cohort, 4-vs-19 contrast, matched seeds, differing event timelines and summaries, plus home goals/shots/pass totals.

Crucially, attributes with NO OBSERVED EVENT CHANGE are explicitly listed in the report. Such a finding means the sampled matches had no detectable effect, not that an attribute is definitively dead. For rarer skills such as reflexes, penalties, corners or injuries, construct a situation that actually triggers the relevant mechanic and rerun with adequate sample size.

Changed event fingerprints show that an intervention altered observable match behavior. They do not establish direction, effect size, realistic outcome or correct opportunity activation by themselves.

## Diagnostic Monte Carlo

The initial baseline comprises 128 90-minute matches with synthetic home attribute-level 14 vs away 12, fixed seeds, no live database matching. This is a diagnostic population ONLY. Targets for goals, shots, pass accuracy and fouls are advisory until realistic fixture sampling is defined. The report and its validator guard arithmetic correctness, finite values and score proportions without artificially requiring unverified realism thresholds.

## Commands

Run from the repository root (first line changes into the Rust crate):

    cd rust/football_match_engine
    cargo test --all-targets
    V4_IMPACT_SEEDS=32 V4_IMPACT_REPORT_PATH=../../build-evidence/v4_attribute_impact.json cargo test --lib v4::attribute_impact_tests::exhaustive_50_attribute_counterfactual_report -- --ignored --nocapture
    V4_REALISM_MATCHES=512 V4_REALISM_REPORT_PATH=../../build-evidence/v4_realism_baseline.json cargo test --lib v4::attribute_impact_tests::monte_carlo_realism_baseline_report -- --ignored --nocapture
    cd ../..
    python3 tools/verify_v4_impact_report.py build-evidence/v4_attribute_impact.json build-evidence/v4_realism_baseline.json

CI publishes the two machine-readable JSON files as an artifact in the existing V4 P2 regression workflow.

## Remaining acceptance gates

- Create controlled, scenario-specific direction/exposure tests for all 50 in-match attributes; do not claim a 50/50 behavioral certification before this is done.
- Test the four career attributes through inter-match systems rather than forcing them into single-shot probabilities.
- Paired confidence intervals on a larger, stratified sample with holdout seeds, results across engine revisions.
- Cross-CPU x86_64/ARM64 determinism, Android JNI stress, runtime throughput and battery tests.
- No production match algorithm or commentary resources are changed in this iteration.


## Initial baseline (GitHub Actions run 45174793834, 23 Nov 2026)

- Rust CI: 63 passed and zero failed at the time of this baseline (before subsequent isolated duel additions).
- Counterfactual 50 x 12 paired seeds: 41 attributes showed distinct event timelines; NINE did not: tackling, marking, corners, determination, leadership, strength, one_on_ones, consistency, big_matches.
- Initial 128 synthetic 90-minute matches: mean goals 0.92, shots 8.23, completed pass rate 63.8%.
- The synthetic sample is not the Premier League and cannot by itself prove a target range violation across all leagues. However, it is an important warning that passing buildup and shot volume likely need model diagnosis before calibration.
- In response, additional controlled tackle, marking, strength and one-on-one keeper tests were introduced. Their passing outcome must be verified on the new CI commit.

Source: https://github.com/tvhuy99-web/Coffee-Shop-Manager-Google-AI-Studio/actions/runs/45174793834

### Diagnostic next steps

1. Count actual opportunities and activations for free kicks, corners, shots on target, fouls, duels and pressure situations. Low exposure makes statistical tests powerless.
2. For the unobserved fields, build named scenario fixtures rather than raising random fixture counts indefinitely.
3. In an independent holdout population, separately measure attacking entries, pass loss locations, buildup sequences, shot propensity and xG. Investigate causes before modifying shot or pass probabilities.
4. Keep release-mode ARM64 throughput measurements separate from debug Linux CI; never extrapolate device performance from a test-runner timing alone.
