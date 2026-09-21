# The fitted pickup gap is a fudge, and it is holding up the calibration

Date: 2026-09-21. **Decision: change nothing in the shipping voicing.** Every
geometry the instrument's own service manual permits scores worse on this
project's objective than the geometry we ship, and shipping a worse-sounding
preset to satisfy a specification sheet helps nobody. What changed is that we
now know why, and the next step is named and falsifiable.

The investigation started from a listening report: soft playing has more
attack edge than a Rhodes does, and on recordings the hammer knock only shows
up when the player hits hard.

## The symptom, measured

Upper-partial content over the fundamental, in the scorer's own 96 ms attack
window, across all 73 notes of the reference at four ordinal layers.

| Register | Error at *p* (model − reference) | Error in the *p*→*f* travel |
| --- | ---: | ---: |
| Bottom octave, 28–39 | −3 to −21 dB, too dark | −4 to +14 |
| Low-mid, 40–50 | −6 to +3, near zero | +1 to **+42** |
| **Middle and upper, 51–95** | **mostly +5 to +15 dB, too bright** | −15 to +10 |
| Top, 96–100 | +7 down to −20, scattered | +20 to +44 |

Over 71 scored notes: error at *p* averages +3.37 dB with an RMS of 8.99 dB;
error in the travel averages +4.49 dB with an RMS of 12.40 dB.

The structural part is the travel. Above A3 the model's *p*→*f* growth **locks
at +13 to +14.5 dB for two and a half octaves**, while the reference falls from
+18 dB and goes negative at the top: on a real Rhodes the highest notes get
proportionally *less* upper content when struck harder, and ours cannot. The
model's velocity-to-brightness slope does not depend on register. The
instrument's depends on it enormously.

## Four explanations, three of them eliminated

**Not the strike weight.** `bar_partial_strike_weight` is the second mode's
shape at the contact point and is used in both directions of the contact
solve. Making it depend on velocity would assert that the hammer lands
somewhere else when the key is hit harder, which is false.

**Not the contact.** Traced hammer contact runs 0.167 ms at *p* and 0.089 ms at
*f*, shortening with the exact exponent the quadratic law predicts, so the
solver is right. Sweeping `contact_stiffness` across **four decades**, 1e8 to
1e12 N/m², moves the attack-over-body travel from −0.69 to −0.65 dB. Inert.

**Not the tine.** Splitting a trace by stage, the tip's own displacement gives
**0.00 dB** of travel from *p* to *f*; the pickup contributes −0.64 and the
output −0.66. The whole velocity dependence is made in the magnet. That agrees
with the measurement literature rather than contradicting it: Muenster and
Pfeifle filmed a Rhodes tine with a high-speed camera and report that after a
very short transient it moves sinusoidally without higher harmonics, and that
the growl comes from position-dependent nonlinearity in the magnetic field,
loudest in the low register where the tips swing widest.

**Not the recording chain.** The library's capture was a Countryman Type 10
active DI into a Chandler TG2, and a TG2 is transformer-coupled, so it
saturates with level — the same shape as the effect being chased. Pooling all
284 reference takes by recorded level separates the two cleanly: at the same
recorded level, around −16 dBFS, the bass carries **+16.1 dB** of upper
partials and the treble **−24.1 dB**. A regression on level alone leaves a
16.95 dB residual; on note alone, 8.55 dB. Forty dB of register separation at
equal voltage is not something a preamp can do. The target is the instrument.

## What is actually wrong

The shipping Calibrated preset runs the aperture pickup at a **0.5 mm gap with
a 0.5 mm offset**, across the whole keyboard.

The [service manual](https://www.fenderrhodes.com/org/manual/ch4.html)
specifies the pickup-to-tine gap between 1/16 in (1.588 mm) and 1/8 in
(3.175 mm). It permits 0.020 in (0.508 mm) only on pianos built after March
1972, and only in the middle and upper ranges — never the bass. It sets the
tine to rest *slightly above dead centre* of the pickup, which is a small
offset, not a whole gap's worth.

At 0.5 mm the finite-aperture flux slope **changes sign about 390 µm out**. The
bottom note's tip swings 1827 µm at *f*, so its waveform inverts polarity
mid-cycle. That is where the +42 and +44 dB travel spikes in the low-mid come
from; it is a numerical artefact of an illegal geometry, not growl. The law's
own test already documents the sign flip as the difference between sitting
inside and outside the pole face. Checked at 1.588, 2.0, 2.5 and 3.175 mm
against offsets of 0.1, 0.25 and 0.5 mm, every gap the manual allows has no
sign change anywhere in the swing.

## Why it cannot simply be corrected

Scored with this project's own objective (H2–H4, attack96 and body) on nine
non-reserved notes spread across the keyboard:

| Geometry | MSE dB² | Against the shipping preset |
| --- | ---: | ---: |
| **Shipping, 0.5 / 0.5 mm** | **181.86** | — |
| Best legal fixed gap, 1.588 / 0.5 | 357.76 | +97% |
| Best legal register law, 1.588 bass → 0.508 treble, 0.5 offset | 271.25 | +49% |
| Manual midpoint, 2.4 / 0.25 | 573.01 | +215% |

No geometry the factory permits — fixed or graded by register — comes close.
Reproduce with `cargo run --release -p rf-tines-lab --example
manual_geometry_probe -- references/audio/matts-fender-rhodes/samples/original`.

So the 0.5 mm gap is not a calibration result. It is the only way this model
can manufacture the upper-partial content the reference has, and it buys that
content by operating the transducer where the instrument cannot be operated.
Every pickup search in this project — [the bounded
search](MATTS-CALIBRATION-SEARCH.md), [the robust
fit](ROBUST-PICKUP-CALIBRATION.md), [the continuous
fit](CONTINUOUS-PICKUP-CALIBRATION.md), [the register
fit](REGISTER-PICKUP-CALIBRATION.md) — bounded the gap below at 0.5 mm, and
every one of them finished sitting on that floor. The optimiser has been asking
to go further out of bounds for as long as there have been receipts.

## The next step, and what it must predict

By elimination the missing content is in the transduction, and the playable
engine's transduction has **one motion coordinate**: the axial aperture
reduction requires a zero vertical offset by construction, and `Voice` carries
a single displacement. A real tine traces an ellipse — Pfeifle models two
transverse polarisations, and the high-speed footage shows non-planar motion
excited by coupling and by hammer-tip imperfection. A tip moving in two
dimensions across a two-dimensional field produces harmonics at modest
amplitude, and produces more of them where the swing is widest, which is the
register dependence the model is missing.

Both pieces already exist here and neither is speculative: the offline action
carries [two-plane tine motion](TWO-PLANE-ACTION.md) over twenty mechanical
coordinates, and `SpatialPickup` already evaluates flux at `[x, y]`. The work
is to carry the second coordinate into the playable voice and give it the
two-dimensional pickup, then refit **inside the manual's box**.

It should be held to this prediction before anything is promoted: with the
second coordinate in place, a fit restricted to gaps of 1.588 mm and wider must
reach an MSE at or below the 181.86 that the illegal 0.5 mm geometry reaches
today. If a legal geometry still cannot match it, the second plane was not the
missing piece either and this document is wrong.

## The prediction was made and it failed

The second coordinate is now in the playable voice, and the prediction above
is **not met**. Scored on the same nine training notes with
`cargo run --release -p rf-tines-lab --example two_plane_fit`, three starts of
deterministic coordinate descent over the manual's own box:

| Voicing | MSE dB² | Against the shipping preset |
| --- | ---: | ---: |
| **Shipping, 0.5 / 0.5 mm, one coordinate** | **181.86** | — |
| Best legal fixed gap, one coordinate | 357.76 | +97% |
| Best legal register-graded gap, one coordinate | 271.25 | +49% |
| **Best legal register-graded gap, two coordinates** | **226.77** | **+25%** |

The ellipse is worth something real: it takes 16% off the best legal
one-coordinate voicing, and it more than halves the penalty for obeying the
service manual. It does not close the gap, so the illegal geometry is still
carrying work that belongs to physics we do not model. The winning fit also
pins both ends of the gap on their floors again — 1.588 mm in the bass and
0.508 mm in the treble — which is the same complaint the optimiser has been
making all along.

So the ellipse was a missing piece and not the missing piece. The next
candidate is the one the measurement literature puts beside it and this model
still does not have at all: the resonator is an asymmetric tuning fork, and we
model only one of its two prongs. Muenster and Pfeifle measure the tonebar's
fundamental several hundred to more than 1400 cents away from the tine's, and
report the two locking into phase or antiphase — a coupled second body with
its own partials, where the playable voice has three bending modes of a single
beam. Before that is built it deserves a prediction of its own, and this one
should be kept on the page as a reminder of what an untested inference is
worth.

## Scope

**No preset, profile or shipping sound changed.** The voice now carries two
transverse coordinates, but `Profile` defaults the boundary angle to zero and
the axis ratio to one, and at those values the second axis takes nothing from
the strike, never moves, and the tip stays on the axis the ten-square-root
reduction assumes. `a_tine_whose_axes_face_the_hammer_never_leaves_the_strike_direction`
holds that to exact zero across the register for all three named profiles. The
two-dimensional flux is the same flux: on the axis it returns what the
reduction returns, which
`the_planar_flux_agrees_with_the_axial_reduction_on_the_axis` checks term by
term.

Added: `tine_boundary_angle_rad`, `tine_transverse_frequency_ratio` and
`pickup_transverse_offset_m` on `Profile`; `PlanarAperture` and
`planar_voltage` beside the axial reduction; `transverse_displacement_m` on
`Probe` and in the render trace; the `manual_geometry_probe` and
`two_plane_fit` examples. Six new tests, each checked against deliberate
mutations of the code they cover.

The layer-to-velocity mapping remains what the frozen protocol says it is — an
assumption, not a measured hammer law — so part of the travel error may be the
mapping rather than the physics, and none of the above should be used to tune
against it.
