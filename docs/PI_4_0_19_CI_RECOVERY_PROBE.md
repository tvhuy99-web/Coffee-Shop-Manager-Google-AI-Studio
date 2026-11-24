# PI 4.0.19 CI recovery probe

Pinned prior head: 6c06d540375ea6b69e1590c074597f7003c8bbc9. On inspection on 2026-11-24, all seven workflows at this head failed. Attribute audit run 45423596759 was created at 15:16:32Z and completed at 15:16:39Z; all six jobs reported steps=null and logs_url=null. Explicit job-step query for 138549425098 returned an empty list, artifacts list was empty, log retrieval returned HTTP 404 BlobNotFound. The local working environment has neither cargo nor rustc and git ls-remote fails DNS resolution for github.com. No successful executable evidence for 4.0.18 or 4.0.19. Last validated policy is 4.0.17 (45410711745).

Add a minimal, independent runner probe: ubuntu-latest, a shell command only, no repository checkout, no third-party actions, no secrets, no dependency setup. If it succeeds, a separate dependent job checks out the exact triggering commit, runs the updated P0 static verifier, sets up Rust and runs existing targeted psychology, manual/AI substitution, checkpoint and deterministic continuation tests. It is not a replacement for the full 904 + 1396 match experiments or full P1/P2/APK workflows. The recovery job's failure at a particular step can be diagnosed from actual logs; do not infer compile failure from an empty job record.

Interpretation:
- No runner marker, no steps/logs: check GitHub Actions run summary annotations and account/repository Actions settings, billing/usage, runner availability; exact cause not known.
- Runner marker exists but checkout/setup fails: diagnose the corresponding step.
- Rust test failure with logs: fix that specific compile or assertion failure.
- All focused tests pass: rerun full audit and DES cohorts before new policy work.

The push trigger is limited to this workflow path, and workflow_dispatch permits a controlled repeat after any account-level issue is resolved. Existing workflows are unchanged, including any that run on every main push. No football policy, source code, PI version, checkpoint schema, test thresholds or statistics changed. This probe does not claim CI is repaired.
