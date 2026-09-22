# Extending the frozen protocol to the attack

## Why

Three decisions in this project were made by an objective that cannot see the
attack, and all three are now known to be wrong or unexamined:

- **The contact was softened** from 4e10 to 4e8 because the objective fell
  41.4%, the largest single improvement in the project. It dropped the
  contact's frequency ceiling from about 4.5 kHz to 0.9 kHz and took the
  attack's high content with it. docs/POLE-THRESHOLD.md has the measurement.
- **The tonebar was switched off** because no coupling improved the objective,
  while docs/PLAYABLE-TONEBAR.md recorded in the same breath that "the
  mechanism works". A Rhodes has a tone bar; it is not optional.
- **A resonance bank was discarded** on a spectral statistic without a
  listening test, when the statistic could not see level at all.

The objective scores H2 to H4 over the `attack96` and `body` windows. At MIDI
52 that is everything below about 800 Hz, averaged over windows far longer
than the fifteen milliseconds a listener calls the attack. Above H4, in the
first 15 ms, and in absolute transient size, it is blind — and every one of
today's seven refuted experiments was fitted against a statistic with one of
those blind spots.

## What is added

Three terms, reported separately and **never summed into one number**. Every
failure in this session came from a scalar hiding what it could not see.

**Term A — harmonic balance. Unchanged.** H2 to H4, `attack96` and `body`,
exactly as it is. Every past figure in every document stays comparable.

**Term B — attack colour.** Third-octave bands from 250 Hz to 11 kHz over the
first 15 ms from the detected onset, each in dB relative to the body's
fundamental region, floored at −90 dB. Mean squared dB error against the
reference. This is the term that sees the 53 to 69 dB hole above 5.7 kHz and
the 17 dB excess at 1 kHz, both measured at MIDI 52 forte.

**Term C — transient size.** The attack's peak over the body's peak, in dB,
squared error against the reference. The reference averages 1.33 across nine
notes at forte and never exceeds 2.27; the shipping model sits at 1.04. This
is the term that would have rejected the `gain * force³` roughness law, which
reached 500 times the clean attack in the bass while scoring well.

## Promotion rule

A change promotes when, on **both** splits:

- at least one term improves by 10% or more, and
- **no term regresses by more than 5%**, and
- no reserved note regresses by more than 10% on any term.

This is deliberately stricter than the rule it replaces. Each of the three
decisions above improved one term while silently wrecking another, and a rule
that reads one number at a time cannot stop that.

## The reserve is spent, and is replaced

The old reserved set — 36, 43, 64, 72, 84, 96 — has been seen in aggregate
across several error maps, and docs/WEDGE-POLE.md says so. Today's work also
measured 28, 30, 38, 50, 52, 55, 59, 64, 67, 76 and 88 directly.

The fresh reserve is **33, 45, 57, 70, 81, 93**: six notes spanning the range,
none of them in the training set and none of them touched today. Nothing may
be fitted, swept, or auditioned on them. The old set is retired to ordinary
use.

## Validation: the terms must catch the known failures

A new metric is worth nothing until it is shown to catch what it was built
for. Before this protocol is used to decide anything, it must reproduce all
three of these, and the results go in the section below:

1. **Softening the contact** must show Term A improving and Term B clearly
   regressing. If Term B does not notice, it is not measuring the attack.
2. **The force-cubed roughness law** must show Term C regressing
   catastrophically.
3. **Switching the tonebar's clamp on** must show Term B improving slightly,
   since it moves 250 Hz five decibels toward the reference.

If any of these three fails, the design is wrong and is fixed before use.

## Validation results

Training notes only. The fresh reserve was not loaded.

| | A balance | B colour | C transient |
|---|---|---|---|
| what ships | 107.37 | 875.4 | 15.2 |
| hard contact (4e10, as it was) | +69.4% | **+39.3%** | −17.9% |
| bank at a gain that breaks it | — | does not score at all | — |
| tonebar clamp 0.8 | −3.0% | **−12.8%** | **−53.0%** |

**Case 1 refuted its own prediction, and the prediction was the wrong way
round.** It said Term A would improve. Returning to 4e10 undoes the 41.4%
improvement that softening bought, so A must get worse, and it does by 69.4%.
The finding is Term B: the hard contact is 39.3% *worse* in attack colour. It
fills the band above 5.7 kHz and overshoots the middle, which the six-band
measurement in docs/POLE-THRESHOLD.md had already hinted at (+6.5 dB at 3 kHz
and +3.3 at 5 kHz at the top of the hardness control). Term B caught it
without being told. The hard contact is not the answer to the attack, and
this contradicts what the session had been drifting toward.

**Case 2 is caught, though not by Term C.** The configuration destroys its own
onset, so nothing scores. That is a rejection and is now reported as one
rather than aborting the run. Term C never gets to speak; the guard fires
first. Both outcomes reject, so the protocol behaves, but the claim "Term C
catches it" is not what was demonstrated and is not claimed.

**Case 3 was not predicted at all.** The tonebar clamp improves every term,
including the original frozen objective by 3.0%. Under the new rule it
promotes: B and C improve by more than 10%, and nothing regresses. Under the
old rule it would not, because A improves by only 3% against a 10% bar.

That is the case this protocol change exists for — a physically correct
component that the old objective could not justify keeping. It is also the
same component the old objective switched off.

### What is not yet established

The reserve has not been spent. Promotion requires the fresh set —
33, 45, 57, 70, 81, 93 — and that is a one-shot measurement, not to be run
while still exploring. Nothing has been promoted. `tonebar_clamp` still
defaults to 0.0.

And the listening test has not happened. Three terms agreeing is a reason to
listen, not a substitute for it; docs/CONTACT-NOISE.md records two separate
occasions in this session where a statistic looked right and the sound was
wrong.
