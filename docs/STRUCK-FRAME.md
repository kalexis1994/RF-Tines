# The struck frame: predictions written before the code

Two failed experiments converge here, and a third measurement decides the
design before any code is written.

## What the two failures said

docs/HIGH-BENDING-MODES.md added the fourth and fifth bending partials. They
landed in the deficient bands and lifted them 19 and 31 dB, and the change was
52 dB under the peak and inaudible. Two tones are not a band.

docs/CONTACT-NOISE.md gated filtered noise with the contact. Fitted on the
spectral statistic it scored well; constrained to the transient size the
instrument actually shows, it contributed nothing. A noise burst is peaky and
brief, and the reference's attack noise is neither.

Both point the same way: something dense in frequency and spread over more
than 20 ms, with no large peak. That is a bank of resonances.

## Why it cannot be the tine

Clamped-free bending modes in 1.5 to 10 kHz:

| note | modes in band |
|---|---|
| MIDI 30 (46 Hz) | 5 |
| MIDI 52 (165 Hz) | 3 |
| MIDI 76 (659 Hz) | **1** |

The tine cannot furnish a dense band in the middle of the keyboard, and
furnishes nothing at all in the treble.

## The measurement that chose the design

If the attack noise belongs to the tine, its spectrum must scale with the
note. If it belongs to a fixed structure — the harp, the frame, the pickup
assembly, all of which the blow shakes and which the magnetic pickup senses as
relative motion against the tine — it must sit at the same absolute
frequencies for every note.

Reference, forte, attack's first 24 ms, each note normalised to its own
maximum, in dB:

| note | 1500 Hz | 3000 Hz |
|---|---|---|
| MIDI 30 (46 Hz) | −0.5 | −20.5 |
| MIDI 38 (73 Hz) | 0.0 | −22.3 |
| MIDI 52 (165 Hz) | 0.0 | −20.4 |
| MIDI 67 (392 Hz) | 0.0 | −22.2 |
| MIDI 76 (659 Hz) | 0.0 | −19.8 |

Across 3.8 octaves of fundamental, the 3 kHz level relative to 1.5 kHz moves
2.5 dB. Tine content could not do that: different modes would land in each
band at each note. The 1500 Hz column proves nothing on its own — four of the
five rows have their maximum there, so it is zero by construction — and the
bands above 4 kHz scatter by 8 to 11 dB, so this holds below about 4 kHz and
is not claimed above it.

The design follows: a bank of **fixed-frequency** resonances, not scaled by
the note, driven by the contact force.

## Predictions, before the code

1. At forte, between-harmonic energy at MIDI 52 rises from −32.8 toward the
   reference's −17.6, landing within 6 dB.
2. At forte the grid ratio falls from 17.1 toward the reference's 6.9.
3. **The attack peak over the body peak stays inside the reference's own
   range** — 1.33 mean, 2.27 worst. This is the constraint both previous laws
   broke, and it is checked at the same time as 1, not after.
4. The frozen objective changes by less than 5%.
5. Cost under 10%.
6. The 750 Hz and 1500 Hz attack bands, already 15 and 8 dB over the
   reference, rise by less than 1 dB. The bank starts above them for that
   reason.

**Failure condition.** If 1 and 3 cannot hold at the same time — if matching
the spectrum again requires exceeding the instrument's own transient size —
then this is not a resonant structure either, and what is left is the
recording chain, which this library cannot separate.

## Result: the bank behaves, and still cannot do it

Twelve fixed resonances from 2.17 to 9.87 kHz, T60 20 ms, driven by the
contact force. Prediction 3 is satisfiable for the first time: the transient
tracks the gain smoothly and there is a gain, 0.0039, where the attack peak
over the body peak is 1.37 against the reference's 1.33.

There, and at every other gain, prediction 1 fails:

| frame gain | p | mp | mf | f | transient |
|---|---|---|---|---|---|
| 0 (clean) | −79.6 | −54.8 | −41.8 | −32.1 | 1.04 |
| 0.0039 | −77.6 | −54.7 | −41.8 | **−32.1** | 1.37 |
| 0.0193 | −67.5 | −53.9 | −41.5 | **−31.9** | 4.16 |

Forte does not move. Not at a legal gain, not at five times a legal gain. Only
the softest layer rises.

## Why, and it applies to all three attempts

The statistic is energy between the harmonics over the fundamental. A linear
resonance driven by the contact force grows exactly as the tone does when the
blow gets harder, so that ratio cannot move with velocity. The same degeneracy
sank the tip-velocity law in docs/CONTACT-NOISE.md, and it is why the force
law there only ever looked right by blowing the signal apart.

What the reference demands, in dB under each file's own fundamental:

| | p | f | rise |
|---|---|---|---|
| reference | −57.7 | −17.6 | **40.1 dB** |
| ours, any linear bank | −79.6 | −32.1 | 47.5 dB fixed by the clean model |

The reference's attack content grows about 40 dB relative to its own tone
between the softest and hardest layer. This is a ratio inside one file, so it
survives any per-layer normalisation the library may have done. No linear
mechanism driven by the blow produces it. It needs a threshold: something that
barely happens at piano and dominates at forte.

The failure condition written above is therefore met for a resonant structure,
and what it points at is not the recording chain — saturation would land on
the harmonics, and this does not. It points at a mechanism with a threshold in
the instrument. The candidates worth measuring, in order of how testable they
are: the tine approaching the pole piece at large excursion, which the profile
already knows the gap for; the hammer tip bottoming out; and the tine touching
its damper on the way past.

Left in the tree, inert by default: the twelve-resonance bank with
rate-correct coefficients and `frame_gain` at zero. The noise generator that
preceded it was removed rather than left dead; docs/CONTACT-NOISE.md keeps
what it taught. Frozen objective 107.37, all 153 tests pass.

## Addendum: the threshold was already in the model

The first candidate was checked next, and it changes where this goes. See
docs/POLE-THRESHOLD.md.
