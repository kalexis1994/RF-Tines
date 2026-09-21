# Matt's reference: bounded pickup and velocity search

Date: 2026-09-15. **Decision: do not promote this candidate.** The existing
Calibrated factory preset and plugin engine remain unchanged.

## Completed block

- Added a reproducible Rust calibration runner with 24 deterministic parameter
  trials, using only D3/G3/B3 at four ordinal dynamic layers for selection.
- Searched pickup gap, pickup offset, maximum hammer speed and velocity exponent
  in log coordinates. Decay and other mechanics were held fixed.
- Froze the winner before evaluating C2, G2, E4, C5, C6 and C7 (MIDI
  36/43/64/72/84/96). These validation notes did not steer the search.
- Compared baseline and winner across 36 source cases, including fundamental
  envelope gates where supported. Renders stay in memory; no audio matrix or
  new installable plugin version was produced.
- Added an automatic promotion decision with explicit missing-data handling
  and six regression tests. The source WAVs are unchanged.

## Result

The winning trial changes pickup gap from 0.5000 to **0.5666 mm**. Pickup offset
remains 0.5 mm, maximum hammer speed 0.8 m/s, and velocity exponent 1.4. These
are model parameters, not recovered measurements of the sampled instrument.

| Split | Calibrated mean squared error (dB²) | Candidate | Reduction |
| --- | ---: | ---: | ---: |
| Training, 12 cases | 58.11 | 48.03 | 17.35% |
| Held-out, 22 scorable cases | 373.95 | 257.22 | 31.22% |

The metric is squared harmonic-balance error, **not a perceptual quality score**.
It uses H2–H4 relative to H1 in attack96 and body350 windows, equal weighting
per available term within each case, then equal weighting per scorable case.
Balances are floored at -60 dB. Missing reference terms are excluded; missing
candidate terms receive the floor. Weak/missing harmonics limit this metric.

| Held-out concert note | Baseline MSE | Candidate MSE | Change |
| --- | ---: | ---: | ---: |
| C2 | 588.14 | 199.46 | -66.09% |
| G2 | 182.98 | 122.63 | -32.98% |
| E4 | 74.03 | 107.97 | **+45.85%** |
| C5 | 601.10 | 493.05 | -17.97% |
| C6 | 192.68 | 161.32 | -16.28% |
| C7, mf/f only | 835.59 | 660.51 | -20.95% |

E4's soft layers improve, but its strong layer gets substantially worse. At
E4/f attack96, reference H2 is -1.53 dB relative to H1; baseline is -17.50 dB
and the candidate is -36.19 dB. The geometry change deepens this mismatch.
This concrete regression rejects a blanket factory gap change.

Nine held-out cases have qualified source, baseline and candidate envelopes.
Their mean absolute slope error increases by only 0.0204 dB/s, below the
predeclared 0.5 dB/s ceiling. No previously qualified source/baseline case
develops a candidate envelope failure. This limited coverage does not certify
the full keyboard's decay: source pitch qualification only supports MIDI 40–90,
and other envelope cases fail their existing gates.

C7/p and C7/mp have no usable reference H2–H4 terms under the spectral method.
They remain explicitly unscored. Incomplete harmonic coverage is an additional
conservative veto; the E4 regression independently rejects promotion.

## Acceptance rule and scope

Before selection, the protocol required at least 10% training and held-out MSE
reduction, no held-out note MSE regression greater than 10%, no newly failed
envelope gate among qualified source/baseline cases, and no mean qualified
envelope error increase greater than 0.5 dB/s. Missing evidence cannot count
as successful validation; zero common qualified envelopes also veto promotion.

This is a bounded local search with three coordinate rounds (log steps
0.5/0.25/0.125), not a global optimum or proof that one shared geometry cannot
work. Positive offsets only and fixed pole radius constrain its scope.
Ordinal p/mp/mf/f are still paired to 0.25/0.45/0.65/0.85. Searching the profile's
speed/exponent does not identify the recording's actual hammer velocities.
Capture gain, source editing and note-off remain unknown. No physical T60 fit,
GUI test or listening conclusion is claimed.

## Reproduction and receipts

The retained [receipt directory](../references/matts-fit-2026-09-15/) contains
the frozen search, protocol, detailed split reports, decision and provenance
hashes. The source bank is identified by the acquisition inventory. The small
standalone runner avoids changing the root SDK dependency lock.

From the workspace in PowerShell:

```powershell
$env:CARGO_INCREMENTAL = '0'
$env:CARGO_TARGET_DIR = Join-Path $PWD 'target'
$env:PYTHONDONTWRITEBYTECODE = '1'
$runner = 'references/matts-reference-runner/Cargo.toml'
cargo build --locked --release --manifest-path $runner --bin matts_fit
$samples = 'references/audio/matts-fender-rhodes/samples/original'
& target/release/matts_fit.exe $samples renders/matts-reference/fit-new
python tools/matts_fit_gate.py renders/matts-reference/fit-new renders/matts-reference/fit-new/decision.json
cargo test --locked --release --manifest-path $runner
cargo clippy --locked --release --manifest-path $runner --all-targets -- -D warnings
python -m unittest discover -s tools -p test_matts_fit_gate.py
```

The output directory and decision file must be new. To validate the frozen
winner without rerunning optimization, pass
`references/matts-fit-2026-09-15/search.json` as the optional third argument to
the executable. That path reuses the original search history and parameters;
it recomputes the spectral and envelope evaluations.

Development runs exposed and corrected unsupported pitch-anchor notes and
missing reference harmonics. The final validation resumed the frozen winner;
neither correction changed selected parameters or used held-out feedback to
retune them. Incomplete runs remain in ignored local render storage.

## Next substantial block

Diagnose the E4 strong-hit even-harmonic deficit and compare velocity-sensitive
pickup behavior across registers. The six validation notes have now been
inspected and must be treated as development data in subsequent fitting;
reserve fresh notes before the next search. Seek improvement in even/odd
harmonic balance together, then reapply envelope and coverage gates before
preparing the standard RackForge audition workflow.

## Follow-up

The search's floor turned out to be the finding. This run, and every pickup
search after it, bounded the gap below at 0.5 mm and finished sitting on that
bound; the instrument's service manual does not permit that geometry at all.
See [the pickup geometry ceiling](PICKUP-GEOMETRY-CEILING.md).
