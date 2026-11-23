# Football Match Engine V4 P2: Own Goals, Statistics & Attribute Regression

## Scope

P2 implements accidental own goals from two defensible situations:

- A defender blocks an attacking shot near their own goal and the deflection enters the net.
- A defender wins an aerial duel from a corner but directs an attempted clearance into their own net.

The risk is small, spatially constrained, and influenced by defender concentration, anticipation, heading and balance. There is no independent random own-goal event or fabricated score.

The event is `OWN_GOAL`, with `side` set to the **beneficiary** and `actor` set to the **defending offender**. Metadata includes `outcome=OWN_GOAL` and `cause=DEFLECTED_BLOCK` or `MISHIT_CLEARANCE`. Regular attacking goals/assists are never awarded for an own goal. After the event, the existing restart mechanism must return the ball to the team that conceded.

## Score and statistics contract

`MatchSummaryV4` maintains team score; `TeamMatchSummaryV4` now includes `ownGoalsFor`, `ownGoalsConceded`, `shotsOffTarget`, `shotsBlocked`, and `saves`. `PlayerMatchSummaryV4` includes `ownGoals`.

Invariants checked by `stats::validate_summary`:

- `shots == shotsOnTarget + shotsOffTarget + shotsBlocked`;
- `passesCompleted <= passesAttempted`;
- `team.goals == sum(player.goals of own team) + ownGoalsFor`;
- `ownGoalsFor(home) == ownGoalsConceded(away)` (and vice versa);
- `ownGoalsConceded == sum(player.ownGoals of that team)`;
- `saves <= opponent shotsOnTarget`;
- possession totals approximately 100%, xG finite/nonnegative.

`stats::audit_result` additionally recounts score, saves, passes and shots from actual **FullEvent** event records, including free kicks, penalties and off-target headers. No corresponding event list is stored in FastBatch, so FastBatch is validated from counters and tested against identical-seed FullEvent fingerprints/summaries.

**Statistics convention:** an off-target attacking attempt redirected into goal by a defender remains a blocked shot for shot accounting; the defender is credited an own goal and the beneficiary team receives a goal, but no attacking player receives a goal or assist. A mishit clearance from a corner may produce a goal without an attacking shot.

## Attribute tests: avoid overstated proof

Rust tests now verify:

1. All 54 fields survive JSON serialization and strictly change the immutable match-input fingerprint when perturbed individually.
2. All 50 in-match fields have at least one direct read in the simulation modules. Four career/meta fields remain separate hooks.
3. An accidental own-goal probability is zero outside the relevant area, lower for stronger defenders, and can produce a real OWN_GOAL event.
4. Previous passing, first-touch, finishing, goalkeeper, fitness, tactical and psychological outcome tests remain in the suite.
5. Dozens of seeded full matches reconcile with event-derived statistics and their FastBatch counterparts.

The direct-source-use test **does not prove measurable causal behavioral impact for each of the 50 fields**. Full controlled, statistically powered per-attribute outcome tests remain future work; mark P2 as fully verified only after those are completed.

## Commands and deployment gates

```bash
python3 tools/verify_v4_p2.py
cd rust/football_match_engine && cargo test --all-targets
```

For Android, also compile the APK with Rust ARM64 and verify TalkBack/TTS independently. P2 changes the statistics screen only; commentary templates and policies are untouched.

Do not merge this work into `main` on the basis of static checks alone. Run the hosted Rust and Android workflows on an isolated integration ref, inspect the status of all jobs and then fast-forward/merge if green.
