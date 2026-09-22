# Contact noise: predictions written before the code

A listener described the reference's attack as astringent, "como una micro
saturación", only at forte, and absent from ours. Three measurements followed,
in this order, and the third is the one that matters.

## What the reference's attack actually is

At MIDI 52, first 24 ms, energy on the harmonic grid against energy halfway
between harmonics:

| | on-grid minus between |
|---|---|
| reference | **3.1 dB** |
| ours | 18.8 dB |

The reference's attack is nearly flat across the grid. That rules out two
things at once. Saturation — of the tine, the pickup or the Chandler TG2 the
library was recorded through — puts energy on exact harmonics, so it would
raise that number, not flatten it. And modes are tones, so adding the fourth
and fifth bending partials did not move it at all; see
docs/HIGH-BENDING-MODES.md for that experiment and its retraction.

It is noise, and it belongs to the instrument rather than to the recording:

| layer | before the note | attack, 24 ms | tail at 1.5 s | fundamental |
|---|---|---|---|---|
| p | silence | −104.6 dB | −115.1 dB | −46.9 dB |
| mp | silence | −91.7 dB | −114.4 dB | −42.6 dB |
| mf | silence | −90.8 dB | −113.9 dB | −40.8 dB |
| f | silence | **−59.4 dB** | −112.3 dB | −41.8 dB |

The tail is flat at −113 dB across every layer, which is that file's noise
floor. The attack at forte stands 53 dB above it. And while the fundamental
moves 5 dB across the four layers, the attack's between-harmonic energy moves
45 dB.

## Where we stand

Same statistic, ours against the reference, in dB under the fundamental:

| layer | reference | ours | gap |
|---|---|---|---|
| p | −57.7 | −76.1 | 18.4 short |
| mp | −49.0 | −55.3 | 6.3 short |
| mf | −50.0 | −41.2 | 8.8 **over** |
| f | **−17.6** | **−32.8** | **15.2 short** |

And the grid ratio at forte: the reference stays flat at 6.9 dB while ours
climbs to 17.1. At forte the reference gets noisier and we get more harmonic.

## The model

The pickup is magnetic, so whatever this is reaches it as tine motion. A
four-mode model cannot represent the tine's high-order, near-field response to
a rough contact — neoprene on steel at high force, with micro-slip. The
standard approximation is to gate a filtered noise source with the contact
force, and that is what will be tried. It is an approximation and it is
labelled one: it stands in for modes that are not resolved, it is not a
derivation.

The contact solve already computes `self.force` every sample.

## Predictions, before the code

1. At forte the between-harmonic energy rises from −32.8 toward −17.6, landing
   within 6 dB of the reference.
2. At forte the grid ratio falls from 17.1 toward the reference's 6.9.
3. At the softest layer the same statistic moves less than 3 dB. The contact
   force there is small, so a force-gated source must stay quiet. If p rises
   as much as f, the gate is doing nothing and this is hiss with a fader.
4. The frozen objective changes by less than 5%. It scores H2–H4 over windows
   far longer than the contact.
5. Cost under 5%: one generator and a one-pole filter, only while in contact.

**Failure condition.** If matching forte requires the softest layer to rise by
more than 6 dB, the force gate is the wrong law, the noise is not contact
driven, and the search moves elsewhere.

## Known weakness of the target

The four reference layers are four takes by a player. Their true hammer speeds
are unknown, so the 32 dB jump from mf to f may be a steep law or simply a
much harder blow than the label suggests. The velocity exponent fitted here
cannot be trusted beyond reproducing these four points, and it should not be
read as a measurement of how a Rhodes responds to velocity.

## Result: the law is wrong, and the encouraging numbers were an artefact

A first law was tried: white noise, one-pole highpassed at 1.5 kHz, amplitude
`gain * force^exponent`, released over 2 ms. Fitted at MIDI 52 it reported

| layer | reference | before | with roughness |
|---|---|---|---|
| p | −57.7 | −76.1 | −71.6 |
| mp | −49.0 | −55.3 | −50.6 |
| mf | −50.0 | −41.2 | −36.0 |
| f | **−17.6** | **−32.8** | **−21.6** |

and the grid ratio at forte fell from 17.1 to 6.4 against the reference's 6.9.
Predictions 1 and 2 appeared to hold.

They did not. Rendered across the keyboard at forte, the attack peak against a
clean attack of about 0.2:

| note | clean | with roughness |
|---|---|---|
| 30 | 0.185 | **106.3** |
| 38 | 0.184 | 48.5 |
| 52 | 0.204 | 25.8 |
| 76 | 0.226 | 1.73 |

Five hundred times the clean attack in the bass. The statistic that looked
good is a ratio between two spectral sums and cannot see absolute level, and
the plugin path limits, so the fit was run on a signal that was already blown
apart. The frozen objective refused to score at all — "no sustained onset" —
which was the first honest signal and was nearly explained away as the onset
detector disliking noise.

The cause is dimensional. `gain * force³` with force in newtons has no natural
scale, and the contact force varies by orders of magnitude from the bass to
the treble, so a gain fitted at one note is meaningless at another. The sweep
only ever looked at MIDI 52.

What survives:

- The characterisation above. The reference's attack noise is real, is the
  instrument's, is 53 dB over that recording's floor and moves 45 dB across
  the layers while the fundamental moves 5.
- Prediction 5, on cost, after one fix. Two `exp` calls per sample cost 17.6%;
  precomputing them and skipping the generator once contact is over and the
  release has run out brought it to 6574 ms against 6578 ms clean, which is
  free.
- The plumbing: the profile fields, the generator, the per-strike seeding, and
  a default of zero so nothing changes until a law is chosen.

What is needed is a law with a natural scale. The candidate is roughness as a
fraction of the tine's own tip velocity, which is dimensionless, scales with
the register by construction and cannot blow up. It has not been tried.

The target's own weakness, recorded above before any of this, still stands and
now matters more: the four layers are not a calibrated velocity sweep, and no
smooth law in force reproduces p −57.7, mp −49.0, mf −50.0, f −17.6.

## Second law: relative to the tip velocity. Also wrong, and it says why.

The replacement has a natural scale by construction: amplitude is a fraction
of the tine's own tip velocity, times the normalised velocity of the blow
raised to an exponent. The first factor carries the register with no per-note
constant; the second is bounded by one. It cannot run away, and across ten
notes at forte it does not: attack peaks stay within a few percent of clean at
the gains where it is quiet, and rise smoothly rather than exploding.

It was also given a decay. The reference's attack noise is still present at
12 to 24 ms, so 2 ms was wrong; the default became 27 ms, the fourth bending
mode's extrapolated T60, since unresolved high modes are what this stands for.

Fitted on the spectral statistic alone, the best point is gain 41, exponent 2,
at 7.9 dB. Three of the four layers land well — p within 1.7 dB and mp within
1.9 — and the one that does not is `mf`, which is anomalous in the reference
itself: it has the loudest fundamental of the four layers and almost the same
attack noise as `mp`.

**But the statistic is blind to level, for the second time.** What the fit
cannot see, measured on the reference at forte over nine notes:

| | attack peak over body peak |
|---|---|
| reference | 1.33 mean, 0.78 to 2.27 |
| ours, clean | 1.06 |
| ours, gain 41 | **6 to 19** |

The winning fit puts a transient five to ten times larger than the instrument
ever produces. Constrained to the reference's own transient size, the gain
lands near 2 — and there the roughness contributes nothing at all:

| layer | reference | ours, clean | ours, gain 2 |
|---|---|---|---|
| p | −57.7 | −76.1 | −75.0 |
| f | **−17.6** | −32.7 | **−32.7** |

Grid ratio at forte stays at 17.1 against the reference's 6.9.

**The two constraints are incompatible, and that is the finding.** A gated
noise burst is peaky and brief. The reference's attack noise is neither: it is
dense across the spectrum, spread over more than 20 ms, and carries no large
peak. Energy that is spread in time without a peak is what a bank of
resonances does, not what a noise burst does.

So the stand-in should be modal after all — but many closely spaced modes
across roughly 2 to 10 kHz with a T60 around 20 ms, which is dense enough to
read as noise on any practical grid, not the two exact bending partials tried
in docs/HIGH-BENDING-MODES.md. Those two were right in frequency and far too
few.

Left in the tree: the profile fields, the generator, the per-strike seeding,
the precomputed coefficients and the early exit, all with a default of zero so
nothing changes. The frozen objective is 107.37 and all 153 tests pass.

## Where the fitter went

`roughness_fit` fitted the two laws above against the reference and produced
every number in this document. Both laws were removed, and the example was
removed with them rather than left in the tree fitting a parameter that no
longer exists. `roughness_range` stays: it measures the attack peak against
the body peak across the keyboard, which is the check that caught both laws
and is not specific to either.
