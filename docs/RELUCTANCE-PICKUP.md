# A pickup derived from the magnetic circuit

Date: 2026-09-21. **Prediction first; the result goes underneath it whatever
it says.** Three predictions have been written this way. The first two failed
and stayed on the page; the third was met and still did not ship.

## Why this, and why now

Every published account agrees on where a Rhodes model is decided. Muenster
and Pfeifle measured the tine moving sinusoidally and located the growl in
the position-dependent nonlinearity of the magnetic field. Sønderbo's 2024
thesis, built with finite differences and nothing in common with this model,
reports that "the pickup model is arguably where the model deviates from the
Rhodes the most", and that what it needs is the physically modelled pickup
Pfeifle preprocessed from a finite-element field.

This project reached the same place by measurement rather than by reading:
[the transducer ceiling](PLAYABLE-TONEBAR.md) found 46% of headroom in the
transduction and none in the resonator, after two resonator hypotheses were
built and measured at a cost of two days.

## What is wrong with the pickup that is here

It treats the tine as a **probe**. The flux is a dipole-like field summed
over a disc of pole face and evaluated at the tine's position, as though the
tine were reading a field it does not disturb. Three things follow, and all
three have been measured:

- The field can **change sign inside the playing range**, which
  [the geometry ceiling](PICKUP-GEOMETRY-CEILING.md) found 390 µm out. That
  is an artefact of summing a source across the direction of travel, not
  something a magnet does to a piece of steel.
- The gap is the **only length scale**, so the only way to get curvature is
  to close it. Every fit this project has run drove the gap to its floor for
  that reason: at 0.5 mm the field profile is narrow against the tine's
  swing and the tine sweeps across a lot of bend, and at the manual's gaps it
  is broad and nearly straight.
- Nothing **saturates**. Real magnetic circuits do.

*(Corrected after the fact: this document first said sensitivity was
unbounded as the gap closes. Measured, it is not — the aperture law's scale
carries `gap^3`, so its peak output across the manual's whole span moves by
0.7x, slightly the other way. The fits went to 0.5 mm for the shape of the
field, not its size. The claim was wrong and is left here corrected rather
than deleted.)*

A steel tine is not a probe. It is a piece of high-permeability material
moving inside the magnet's circuit, and what changes is the circuit's
reluctance.

## What is being built

Flux through the coil is the magnet's magnetomotive force divided by the
reluctance of the path it drives:

```text
Phi(u) = 1 / (floor + r(u))
r(u)   = sqrt(1 + (u/gap)^2) * (1 + (u/width)^2)
```

`u` is the tine's lateral position relative to the pole axis, `gap` the axial
standoff, `width` the pole's half-width, and `floor` the part of the circuit
that is iron and does not move. The first factor is the path getting longer
as the tine slides off the axis; the second is the overlap shrinking once it
passes the pole's edge. Voltage is `-N dPhi/dt`, so the audio is
`-Phi'(u) * u_dot`, and `Phi'` has a closed form.

Three consequences are structural rather than fitted. `Phi` is monotone in
`|u|`, so **the slope has exactly one zero, at the pole axis, and can never
invert**. The pole's `width` is a **second length scale**, so curvature can
come from a narrow pole at a legal gap instead of only from closing the gap,
which is the thing that has forced every fit out of bounds. And `floor`
**saturates** the circuit as the tine approaches, which is a compressive knee
a dipole does not have.

## The prediction

**Primary.** A fit confined to the service manual's gaps — 1.588 mm and wider
in the bass — must reach **at or below 181.86 dB²** on the nine training
notes, *without* the wedge and *without* the tonebar. Same bar and same cases
as the three attempts before it, and this time with two of them switched off,
because if the circuit model is right it should not need either.

**Secondary, and structural.** No geometry in the validated ranges may put
more than one zero of the flux slope inside the swing the keyboard reaches.
Unlike the wedge, where this had to be arranged, here it should be impossible
to violate.

**Third, and this one is new.** The winner must pass
`a_voice_rings_at_one_pitch_and_not_two`. The last candidate reached 73.07
dB² and was rejected on hearing because it rang at two pitches 78 cents
apart, which the objective cannot see. The guard exists now and the fit is
held to it, not just the test afterwards.

**What is not predicted.** That it beats 73.07. That number came from a
candidate that was audibly broken, and a model that is merely right does not
have to beat a model that was cheating.

## What is assumed

| Quantity | Where it comes from |
| --- | --- |
| Flux through a circuit is MMF over reluctance | Standard magnetics |
| A steel tine lowers the reluctance of the path it enters | Standard magnetics |
| Path length grows as `sqrt(gap^2 + u^2)` | Geometry of the shortest air path |
| Overlap falls as `1/(1 + (u/width)^2)` | **Assumption.** A smooth stand-in for the pole face's finite extent, not a computed area. |
| The iron fraction, `floor` | **Assumption**, and the parameter a fit moves. |

Pfeifle preprocessed the real field with finite elements. This is not that,
and it should not be read as that: it is a lumped magnetic circuit with two
shape assumptions, chosen because it is derivable, cheap enough for 73 voices
and structurally incapable of the artefacts the present law produces.

## Three premises of this document, falsified before the fit ran

Writing the reasoning down first is only worth anything if the reasoning is
then checked, so it was, and most of it did not survive.

**"Sensitivity is unbounded as the gap closes."** False. The aperture law's
scale carries `gap^3`, and its peak output across the manual's whole span
moves by 0.7x — slightly the other way.

**"The fits went to 0.5 mm because that is where the curvature is."** Also
false, at least as a general statement. Driven with a 450 µm sine, the
dipole makes −6.6 dB of harmonics at 0.5 mm and −5.5 dB at the manual's
1.588 mm: the lawful gap is the more nonlinear of the two. Whatever the
small gap buys, it is not curvature at a mid-register swing. The likeliest
remaining explanation is the sign inversion, which is a violent nonlinearity
and only reachable by the largest swings, but that is now a hypothesis and
not a claim.

**"Iron bounds the gap's leverage by a lot."** It bounds it by a little:
1.8x against 2.3x. Real, and far smaller than asserted.

What survives is the one thing checked by exhaustion rather than by
argument: over 900 geometries spanning every validated range, the circuit's
flux slope crosses zero exactly once, at the pole axis, and cannot be made to
invert. `the_circuit_law_has_one_zero_and_no_geometry_adds_another` holds it.

The rest of the case for the circuit now rests entirely on the primary
prediction, which is the only part that was ever going to decide it.

## The result

**All three predictions are met, and it still does not pass the gate.**

| | Training | Held out |
| --- | ---: | ---: |
| Shipping, illegal dipole geometry | 181.86 | 373.95 |
| Second motion coordinate | 226.77 | — |
| Tonebar | 226.34 | — |
| Wedge (unlawful in a different way: rang at two pitches) | 73.07 | 205.47 |
| **Magnetic circuit** | **60.31** | **177.34** |

**Primary: met.** 60.31 on the training notes, −66.8% against the bar, with
the bass gap at the manual's own 1.588 mm minimum and **neither the wedge nor
the tonebar** — as predicted, it needed neither. On the reserved notes it
holds −52.6%, the best any candidate here has reached.

**Secondary: met, by exhaustion.** 900 geometries across every validated
range, one zero each, none inverting.

**Third: met.** `a_voice_rings_at_one_pitch_and_not_two` passes at five notes
across the range. The candidate a listener rejected does not. This is the
first candidate to clear the guard that the objective cannot see.

### Why it still does not pass

Two reserved notes regress: 64 by 23.1% and **84 by 137.2%**.

| Note | Shipping | Circuit | Change |
| --- | ---: | ---: | ---: |
| 36 | 588.14 | 68.82 | −88.3% |
| 43 | 182.98 | 118.27 | −35.4% |
| **64** | 74.03 | 91.16 | **+23.1%** |
| 72 | 601.10 | 158.50 | −73.6% |
| **84** | 192.68 | 456.96 | **+137.2%** |
| 96 | 835.59 | 163.38 | −80.4% |

Nothing ships. The reluctance law is off in every profile.

### Note 84 is the same failure twice

The wedge regressed on 43 and 84; the circuit regresses on 64 and 84. **84 is
in both**, and it is the worst in both. Two candidates built on different
physics fail in the same place, which makes it a property of the model rather
than of either candidate.

That place is already on record. The register map taken at the start of this
work found the model's velocity-to-brightness slope locking flat at +13 to
+14.5 dB from A3 upward while the reference falls from +18 and goes negative
in the top octave. Note 84 is C6. The treble was wrong before any of this
began, and improving the transducer has made the rest of the keyboard good
enough that the treble is now what blocks the gate.

The gap grading across the register is the obvious suspect and is the least
justified part of the geometry: it is a smoothstep between two ends, chosen
because the manual allows the treble to close and the bass not to. Nothing
measured says the real grading is smooth or monotone.

## Where this leaves the model

The transducer question is settled as far as this work can settle it. Three
laws were tried and measured against the same bar on the same cases:

- The **dipole disc** that ships needs a geometry the factory forbids, and
  inverts inside the bass swing.
- The **wedge** removes the inversion and reaches 73.07, and rang at two
  pitches.
- The **circuit** reaches 60.31, holds −52.6% on reserved notes, keeps to
  lawful gaps, needs no other change and rings at one pitch.

What remains is not the pickup. It is the treble, and the register grading
that nobody has justified.

Three of this document's mechanism claims were wrong, including the one
about iron, which the fit then set to zero and never used. The law works;
the story about why it works did not survive contact, and the parts that
did are the two that were checked rather than argued.

## The treble is not demonstrably broken, and the reserve is spent

Note 84 failed in two candidates, so the treble looked like the next thing to
fix. It was diagnosed, and the diagnosis says something else.

**What the failing notes showed.** Across 64, 84 and 96 the model has far too
little second harmonic and far too much third — a symmetry signature, because
a transfer that is odd about the tine's resting point makes odd harmonics and
no even ones. Walking the resting offset on those notes moved the pair in
opposite directions and found a balance point for each: 64 wanted 0.12 mm
(−43%), 84 wanted 0.5 mm (−71%). The optima were not monotone across the
register, so no smooth grading covers them.

**What the training notes show.** Nothing. The same sweep over the nine
training notes finds the global 0.2727 mm already optimal for seven of them,
including every treble note among them — 67, 76, 88 and 98 all gain exactly
zero. Only the two bass notes prefer a different value, at 0.5 mm, and they
gain 27% and 29%.

So the treble defect **does not reproduce on the data this work is allowed to
use**, and the per-note offsets that fixed 64 and 84 were fitting the reserve
with four data points each. That is the thing the frozen protocol forbids,
and it was done here before it was noticed: the sweep was run on three
reserved notes because they were the ones failing. **The reserve is spent for
diagnosis as well as for fitting, and the next evaluation needs notes nothing
in this work has touched.**

**A hypothesis the data supports and this model should not chase.** Tines on
a real instrument are regulated one at a time and they are not identical; the
sample library's own notes warn that one of its tines is dead and "adds some
character". Notes 64 and 84 may be that instrument's own per-tine variation
rather than the model's error. A global model cannot reproduce it and should
not try, and the per-note rule in the promotion gate cannot tell the two
apart.

That is an argument about the gate, and it is not one to act on while holding
a candidate the gate rejects. The rule is what has kept this honest; the time
to revisit it is not the moment it is inconvenient.

## Where this stops

The circuit law is the best model this work has produced, by every measure
that was agreed in advance, and it is not promoted. What would decide it is
not more physics:

- A **fresh reserve** — around 58 of the 73 notes have never been fitted on,
  though all were seen in aggregate in the register map.
- A **converged fit** on training notes; the search was three coarse starts.
- And a **judgement** about whether a per-note rule is the right gate for a
  model that deliberately does not fit per-tine regulation.

The bass offset is the one legitimate, training-supported improvement found
here and it is left unapplied, because applying one parameter from an
unpromoted candidate is how a voicing drifts.
