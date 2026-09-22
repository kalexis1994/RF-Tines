# High bending modes: predictions written before the code

A listener, at MIDI 52 forte, judged `portable-bark-1972` the closest of the
eight presets to the reference and said it "le falta ese pop inicial". That is
measurable, and it was measured.

## What was measured first

The attack's first 15 ms, in octave bands, in dB under the tone that follows,
against the reference recording, on training notes 38/50/55/59:

| band | shipping (hardness 0.45) | hardness 0.95 |
|---|---|---|
| 750 Hz | **+14.8** | +15.0 |
| 1500 Hz | **+8.0** | +8.4 |
| 3000 Hz | −4.9 | +6.5 |
| 5000 Hz | −17.5 | +3.3 |
| 7500 Hz | **−46.8** | −25.5 |
| 10 kHz | **−54.0** | −41.0 |

Two readings. The missing pop is above 7.5 kHz, and `hardness` cannot reach
it: at the top of the control we are still 25 and 41 dB short. And the two
bands that are already 15 and 8 dB *over* the reference do not move with
`hardness` at all, so turning it up trades balance for a fraction of the pop.
That is what produced the tick the same listener rejected earlier.

## The hypothesis

The pickup is magnetic. It cannot hear the hammer; it hears only the tine. So
the reference's attack above 7.5 kHz must be the tine's own high bending
modes, which a short contact excites and which decay in milliseconds.

`MODES = 4` carries three bending modes and the tonebar. The clamped-free
ratios are 1 : 6.267 : 17.55 : 34.39 : 56.84, and the model stops after the
third. At MIDI 50–59 the fourth lands at 5.2–7.1 kHz and the fifth at
8.6–11.7 kHz — the two deficient bands exactly.

## Predictions, before the code

1. Adding the fourth and fifth bending modes lifts the 7.5 kHz and 10 kHz
   attack bands by at least 15 dB toward the reference.
2. The 750 Hz and 1500 Hz bands move by less than 1 dB. Nothing new lives
   there. If they move, the new modes are arriving through the pickup's
   nonlinearity rather than carrying their own content, and the experiment
   has not tested what it claims to.
3. The frozen objective changes by less than 5%. It scores H2–H4, which at
   these notes is below 1 kHz, and the new modes are above 5 kHz.
4. `COORDINATES` goes 8 → 12, so the per-sample integration grows by about
   half. Much more than that means something else is scaling with it.

**Failure condition.** If both modes are in and the 7.5 kHz and 10 kHz bands
are still worse than −20 dB against the reference, modal truncation is not
the explanation and the search moves to the transduction.

## What is derived and what is assumed

Derived. The frequency ratios are `(β_n L)²` with the standard roots, exact
to the three the model already uses.

Derived, with a caveat. The strike vector is the mode shape at the strike
point. Fitting the shipped `[1.0, -0.3, 0.12]` to a clamped-free beam lands at
x/L = 0.870 with a residual of 0.038, which is a good fit, and that position
gives 0.371 and −0.651 for the fourth and fifth. Those are used.

**Not derived, and a finding in its own right.** `PICKUP_WEIGHTS` is
documented as the mode shape at the tip, but no position on a clamped-free
beam produces `[1.0, 0.8, 0.6]`: the best fit leaves a residual of 1.92 and
wants `[1.0, -0.380, -0.125]`. A real cantilever alternates sign near the tip;
the shipped vector is positive and monotone, which is not a beam. The three
shipped values are left exactly as they are, because changing them is a
different experiment with a different blast radius. The new entries take the
beam's magnitudes at the same position, 0.535 and 0.777, which are the same
order as the values beside them. This is the weakest link in the change and
it is the first thing to suspect if prediction 1 fails.

Assumed. The new decays extrapolate T60 ∝ f^−p with p = 1.037, the exponent
the shipped second and third partials already imply between themselves
(0.16 s at 6.267×, 0.055 s at 17.55×). That gives 0.0274 s and 0.0163 s.

## Result

Same measurement, same notes, shipping hardness 0.45:

| band | before | after | change | predicted |
|---|---|---|---|---|
| 750 Hz | +14.8 | +14.8 | **0.0** | under 1 dB |
| 1500 Hz | +8.0 | +8.0 | **0.0** | under 1 dB |
| 3000 Hz | −4.9 | −2.9 | +2.0 | — |
| 5000 Hz | −17.5 | −5.3 | +12.2 | — |
| 7500 Hz | −46.8 | −27.9 | **+18.9** | at least 15 dB |
| 10 kHz | −54.0 | −22.8 | **+31.2** | at least 15 dB |

Overall attack distance 34.3 → 22.2 dB.

- **Prediction 1 holds.** Both deficient bands lift by more than 15 dB.
- **Prediction 2 holds, exactly.** The two over-full bands do not move at all,
  to the tenth of a decibel. The new content is the modes' own; nothing is
  arriving through the pickup's nonlinearity.
- **Prediction 3 holds.** The frozen objective goes 107.37 → 107.14 dB², which
  is −0.2%, against a 5% limit.
- **Prediction 4 holds.** A fixed render workload goes 6620 ms → 8985 ms, +36%,
  against an expected 50%.

**But the failure condition is not cleared.** It asked for the residual to be
better than −20 dB, and 7.5 kHz sits at −27.9 and 10 kHz at −22.8. Two
criteria were written and they disagree: the predicted lift arrived, in the
predicted bands, with nothing moving where nothing should, and the deficit is
still 23 to 28 dB.

The honest reading is that modal truncation was a large part of the cause and
not all of it. It is not licence to keep adding modes until the number is met;
the sixth bending mode is at 92.5× and leaves the audible band across most of
the keyboard.

One candidate for the residual, which this recording cannot settle. The
listener describes the reference's attack as astringent, "como una micro
saturación", and only at forte. The reference was taken through a Chandler
TG2, a transformer-coupled preamp that saturates on transients. Some of that
character may belong to the recording chain rather than to the instrument, and
a single DI'd library cannot separate the two. Chasing it in the tine model
would be fitting the model to a preamp.
