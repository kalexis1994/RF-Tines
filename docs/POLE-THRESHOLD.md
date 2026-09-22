# The pole threshold: the mechanism was already there

docs/STRUCK-FRAME.md ended needing something with a threshold — quiet at
piano, dominant at forte, landing between the harmonics. Three added sources
had failed, all for the same reason: anything linear driven by the blow grows
exactly as the tone does, so it cannot change a ratio against the tone.

## The prediction, and why it was wrong

Written before the measurement: whatever the tine does near the pole it does
once per cycle, so it lands on the harmonics, and the content being chased
lands between them.

That reasoning is wrong, and the model says why. The partials are at 1, 6.267
and 17.55 — not integers. A nonlinearity mixing them produces sums and
differences at 5.267, 11.28 and so on, none of which are harmonics of the
fundamental. The pickup's fold puts energy *between* the grid precisely
because the tine is not harmonic.

## What the tine actually does

Tip excursion against the shipping pickup gap, forte:

| note | gap | piano | forte | forte / gap |
|---|---|---|---|---|
| 30 | 500 µm | 452 | 2498 | **5.00** |
| 52 | 500 µm | 156 | 861 | **1.72** |
| 59 | 500 µm | 111 | 610 | 1.22 |
| 76 | 500 µm | 52 | 260 | 0.52 |

At forte the tine swings one to five times past where the pole piece is,
across most of the keyboard, and at piano it stays well inside. A real tine
cannot pass through the pole. This is the threshold, it has the right
velocity dependence by construction, and it is not something to add — it is
already running.

## Sweeping the gap, with the transient watched this time

MIDI 52, between-harmonic energy in the attack in dB under the fundamental.
Reference: p −57.7, mp −49.0, mf −50.0, f −17.6, grid ratio at forte 6.9,
attack peak over body peak 1.33.

| gap | p | mp | mf | f | grid at f | transient |
|---|---|---|---|---|---|---|
| 500 µm (ships) | −79.5 | −54.8 | −41.8 | −32.2 | 26.0 | 1.04 |
| 700 µm | −72.2 | −53.0 | −40.9 | **−25.7** | 17.2 | 1.03 |
| 1000 µm | −62.4 | −40.2 | −30.3 | −36.6 | **3.6** | 1.06 |
| 1588 µm (manual min) | −82.0 | −70.9 | −54.6 | −46.1 | −13.4 | 1.05 |
| 3175 µm (manual max) | −86.8 | −79.0 | −55.9 | −47.0 | −14.7 | 1.04 |

Three things no previous attempt managed.

The grid ratio moves, from 26 dB down through the reference's 6.9. This is
the first mechanism that makes the attack flat across the grid at all.

**The transient never moves**, staying at 1.03 to 1.06 whatever the gap. Both
earlier laws could only buy spectrum by paying in transient size; this one
does not.

And the layer pattern is non-monotonic — at 1000 µm, `mf` is louder than `f`.
The reference is non-monotonic too, with mp −49.0 above mf −50.0, which was
put down to the library being four takes by a player. A fold whose crossing
point moves through the swing produces exactly this kind of reversal. That is
a hypothesis, not a result, and it is testable.

## The tension this opens, which is the real finding

No single gap reproduces the four layers, and the useful behaviour lives in a
narrow, non-monotonic band around 700 to 1000 µm.

Worse, that band is outside the service manual. The manual specifies 1.588 to
3.175 mm, and 0.508 mm only on pianos after March 1972 and only in the middle
and upper ranges. At the manual's own minimum the mechanism disengages
entirely: forte falls to −46.1, twenty-eight decibels under the reference.

At 1588 µm our forte excursion of 861 µm is 0.54 of the gap, which is the
regime a real instrument plays in. There the model produces almost none of
this content. So the flux law is too gentle at half-gap excursion: the
shipping profile compensates by running the gap three times tighter than the
factory allows, which is how the model gets any of this character at all.

That puts the question back on the transduction law, where
docs/PICKUP-GEOMETRY-CEILING.md already put it by a different route. The next
measurement is not another source to add. It is whether any of the four
pickup laws in the tree produces the reference's attack content at a gap the
manual permits.

## The four laws at the manual's gaps: none of them does it

That next measurement was run. MIDI 52, same statistic, all four pickup laws
across the manual's box, with the shipping 500 µm kept as a reference point
even though it is outside the specification.

| law | gap | p | mp | mf | f | grid at f | transient |
|---|---|---|---|---|---|---|---|
| Production | 500 | −85.1 | −62.4 | −49.0 | −41.7 | 16.0 | 1.03 |
| Production | 1588 | −87.1 | −69.8 | −55.3 | **−47.1** | −7.4 | 1.04 |
| Production | 3175 | −87.2 | −77.8 | −55.9 | **−47.1** | −13.8 | 1.04 |
| Aperture | 500 | −79.5 | −54.8 | −41.8 | −32.2 | 26.0 | 1.04 |
| Aperture | 1588 | −82.0 | −70.9 | −54.6 | **−46.1** | −13.4 | 1.05 |
| Aperture | 3175 | −86.8 | −79.0 | −55.9 | **−47.0** | −14.7 | 1.04 |
| RegisterAperture | 1588 | −82.0 | −70.9 | −54.6 | **−46.1** | −13.4 | 1.05 |
| Reluctance | 500 | −83.7 | −63.6 | −51.6 | −44.3 | **10.1** | 1.03 |
| Reluctance | 1588 | −87.8 | −73.3 | −55.8 | **−47.3** | −10.0 | 1.04 |
| Reluctance | 3175 | −87.7 | −75.6 | −55.9 | **−47.2** | −11.9 | 1.04 |

Reference: f −17.6, grid ratio 6.9, transient 1.33.

**No law produces it at a gap the manual permits.** At 1588 µm and above, all
four land forte between −46.1 and −47.3, twenty-nine decibels under the
reference, and they converge there: at a legal gap the four laws are
indistinguishable, because none of their nonlinearities engages at the
excursion a real instrument reaches. Whatever separates a dipole from a
magnetic circuit in this model, it only exists at a gap the factory does not
allow.

Two details worth keeping. `RegisterAperture` is identical to `Aperture` at
every gap here, which is not a fault: `for_note` clamps its register weight to
zero below MIDI 55, so at MIDI 52 the adjustment is switched off by design,
and this note does not test it. And at the shipping 500 µm the laws do differ,
with `Reluctance` giving a grid ratio of 10.1, the closest any configuration
has come to the reference's 6.9 — but its forte sits at −44.3, so it is flat
and empty rather than flat and present.

## What this establishes

The model's entire attack character is bought by running the pickup gap three
times tighter than the service manual allows. Put the gap where the factory
puts it and all four transduction laws go quiet together.

So the deficiency is in the shape of the flux law, not in the choice among the
four. A real pickup's nonlinearity has to engage at about half the gap, which
is where a real tine swings at forte; all four of ours need one and a half
times the gap to do anything. That is a claim about Phi(x) that magnetics can
settle, and it is the same conclusion docs/PICKUP-GEOMETRY-CEILING.md reached
from the harmonic-balance side.

Nothing was changed. `pickup_gap_m` still ships at 500 µm, the frozen
objective is 107.37 and all 153 tests pass.
