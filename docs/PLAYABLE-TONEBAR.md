# The tonebar in the playable voice

Date: 2026-09-21. **The prediction is written first, before the code, and the
result is recorded below it whatever it says.** The last time this project
predicted what a missing piece of physics would buy, the prediction failed and
[stayed on the page](PICKUP-GEOMETRY-CEILING.md); that is the point of writing
it down.

## Why the tonebar

The resonator is an asymmetric tuning fork and the playable voice models one
of its two prongs: three bending modes of the tine, and nothing else. The
service manual describes the two prongs as a fork and reports the loss of
sustain when the tonebar is restrained, which makes the tonebar a reservoir
the tine trades energy with rather than an ornament. Muenster and Pfeifle
measure the two prongs' fundamentals several hundred to more than 1400 cents
apart, and describe the tine forcing the more heavily damped tonebar into
phase or antiphase with its lowest eigenfrequency.

The reason this matters now is not the tonebar's own partial. It is that
[the pickup geometry ceiling](PICKUP-GEOMETRY-CEILING.md) showed this model
manufactures **every** non-fundamental component in an over-driven pickup,
which is why its only workable geometry is one the factory forbids. A
resonator that supplies content of its own is the standing candidate for
letting the pickup back off.

## The prediction

**Primary, and the one that decides this.** With the tonebar in the playable
voice, a fit confined to the service manual's gaps — 1.588 mm and wider in the
bass — must reach a harmonic MSE **at or below 181.86 dB²** on the nine
training notes: the score the illegal 0.5 mm geometry reaches today. This is
the same bar, on the same cases, that the second motion coordinate failed at
226.77.

**Secondary, and falsifiable on its own.** The tonebar should change the
*shape* of the decay, not only its speed. The fundamental currently decays as
one exponential because nothing takes energy from it and gives it back. With a
coupled second prong, the envelope of a held note must stop being a straight
line in dB: measured over the first two seconds at G3, the residual of a
straight-line fit to the fundamental's envelope must grow by at least a factor
of two against the uncoupled voice.

**What is not predicted.** The tonebar is not expected to reduce the model's
positive harmonic bias by itself. Today the model carries +8.22 dB too much
upper content in the body and +10.53 dB at the second harmonic; adding a
resonator adds content rather than removing it. Any reduction has to come from
a refit that backs the pickup off, which is what the primary prediction tests.

## What is being built

One coupled pair, replacing the single fundamental. The tine's first bending
mode and the tonebar's lowest mode are joined by a spring standing for the
aluminium block, and the pair's two normal modes take the fundamental's place
in the modal set. Both are heard through the tine, because the hammer strikes
the tine and the pickup watches the tine: the tonebar reaches the output only
by moving the root its neighbour is clamped to.

This costs the solver nothing. Normal modes are independent oscillators, so
the existing contact solve and modal advance carry them unchanged; what the
coupling buys — beating, energy traded between prongs, a decay that is not one
exponential — falls out of superposing two modes that damp at different rates.

At zero coupling the pair separates exactly: the tine mode is the fundamental
it always was, and the tonebar mode has no participation at the tine, so it is
neither driven nor heard. Every voicing that predates this control renders
bit-identically, and a test holds that.

### What is sourced and what is assumed

| Quantity | Where it comes from |
| --- | --- |
| The two prongs are a fork, and restraining the tonebar costs sustain | Service manual, chapter 1 |
| Fundamentals several hundred to >1400 cents apart | Muenster and Pfeifle, ISMA 2014 |
| The tonebar is the more heavily damped prong | Muenster and Pfeifle, ISMA 2014 |
| Tonebar effective mass relative to the tine | **Assumption.** No measurement here or in the sources. |
| Coupling stiffness | **Assumption**, and the control a player moves. |
| Modal damping mixed by energy share | **Assumption**, the usual proportional-damping reduction. |

The bass prong of a real fork is heavier and the brass bar is heavier than the
steel wire, so a mass ratio above one is the direction the physics gives;
its size is not measured and should not be read as one.

## The result

**The primary prediction failed.** Held at the best legal geometry the
previous work reached, joining the tonebar is worth **−0.2%**: 226.34 dB²
against 226.80 with it switched off, and every other setting across five
spacings, five couplings and two decay times is neutral or worse. The bar was
181.86. Reproduce with `cargo run --release -p rf-tines-lab --example
tonebar_verdict -- references/audio/matts-fender-rhodes/samples/original`.

**The secondary prediction was half right, and the half that failed is the
informative one.** The mechanism works: where the coupling draws the two
normal modes close, the envelope stops being a straight line and leaves a
3.709 dB residual against 0.108 dB for a lone tine — a thirty-fold bend. But
at the spacing the measurements actually report, several hundred cents and
more, the prongs are too far apart to trade and the envelope stays as straight
as before: 0.112 dB against 0.108. The reservoir exists in the model and the
real fork is not tuned to use it.
`a_joined_tonebar_bends_the_decay_it_used_to_be_a_straight_line` holds both
halves, so neither can quietly change.

There is a third finding the predictions did not anticipate. In this model the
second prong **shortens** the note — the decay slope goes from −1.86 dB/s alone
to −7.81 at the setting that bends hardest — because a more heavily damped
neighbour is a place for energy to go and not come back. The service manual
says the opposite of the real instrument: restraining the tonebar *costs*
sustain. A tuning fork rings because its prongs move in antiphase and cancel
the force into the stem, so nothing leaks into the mount. That is momentum
balance at the root, and it needs the support coordinate the offline
[assembly](COUPLED-ASSEMBLY.md) has and the playable voice does not. What is
built here is the wrong mechanism for sustain, and it is written down rather
than quietly left.

### The bug the first verdict was actually measuring

The first run of the verdict reported the tonebar making the fit four to five
times worse, which was not the tonebar. Joining a spring to the tine stiffens
it, so the assembled note came out sharp: 83 cents at a tenth of the tine's
own stiffness, a full octave at three times it. The scorer was reading a
detuned instrument.

A real tine is tuned after it is assembled, with its tuning spring, so the
model now retunes the pair onto the note — and onto whichever normal mode is
actually heard, because joined hard enough the prongs stop being a tine and a
tonebar and the other one leads.
`joining_the_tonebar_leaves_the_note_where_it_was` holds every note in the
range to within 35 cents at any coupling. Without that fix this document would
have recorded a confident, wrong conclusion.

## What this means for the next step

Two resonator hypotheses have now been predicted, built and measured. The
second motion coordinate took 271.25 to 226.77; the tonebar takes it to
226.34. Neither reaches the 181.86 an illegal pickup geometry reaches, and the
tonebar reaches nothing at all.

That is evidence about the class of explanation, not only about two
candidates. The model's missing upper-partial content is **not** a missing
resonator, and the measurement literature said so from the start: Muenster and
Pfeifle report the tine moving sinusoidally without higher harmonics, and the
growl being made in the position-dependent nonlinearity of the magnetic field.
We have now twice added resonator physics to a problem the sources locate in
the transducer.

So the next candidate is the transducer's own shape. The same paper describes
the pole as **wedge shaped, pointing at the tine**; this model uses a disc, a
sixteen-node quadrature over a circular pole face. Pole radius alone is worth
5.07 dB of spectral distance, which makes the pole's *shape* a larger lever
than its size, and it is the one part of the growl mechanism nobody here has
questioned. It deserves its own prediction, written before the code, and it
may fail like these two did.

## Kept, and what it costs

The tonebar stays in the voice. It is off in every shipping profile, it is
physically real, it is the mechanism a later support coordinate would build
on, and it is free: 73 voices at 128 frames measure p99 1.804 ms against
1.840 ms before it, with the same six deadline misses out of 1125 blocks. What
it is not is an improvement to the model's accuracy, and no preset claims one.

## The experiment that should have come first

Two builds were spent finding out that two resonator hypotheses do not help.
Both could have been ruled out in an afternoon by asking the question one
level up, so that is what was done before starting a third: **is there any
transducer at all that would do?**

The pickup has no back-action on the tine, so the mechanics can be rendered
once and the transducer swept over it for nothing. Whatever the pole looks
like, it turns tip motion into `-g(x) v` for a single function `g` of
displacement. Fitting `g` freely — a piecewise-linear curve over the swing the
whole keyboard reaches, with no physics imposed on it — bounds what every
possible pole can reach. `transducer_ceiling` does that.

| | MSE dB² on the training notes |
| --- | ---: |
| Second motion coordinate, best legal geometry | 226.77 |
| Tonebar, best of fifty settings | 226.34 |
| **Shipping geometry, the illegal one** | **181.86** |
| The shipping curve resampled at the probe's knots | 155.82 |
| **A freely shaped transduction curve** | **98.03** |

The transducer has roughly **46% of headroom** where both resonators had
none. That is the direction.

Two things have to be said against it. The probe reproduces its control point
to −14.3%, not exactly: fifteen knots 286 µm apart interpolate the real curve
coarsely, and that coarseness is itself worth 14% — which is its own finding,
because smoothing the disc pole's sign inversion helps. And the curve the
search lands on changes sign five times across the swing, which no magnet
produces. So 98.03 bounds *memoryless transducers*, not *physical poles*, and
a wedge will land somewhere between it and 181.86.

What the number does establish is that the content the model is missing is
reachable through the transduction and is not reachable through the
resonator. The sources said so; it took two builds and this probe to believe
them.
