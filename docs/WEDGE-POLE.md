# A wedge-shaped pole

Date: 2026-09-21. **Prediction first, result recorded below it whatever it
says.** Two predictions have now been written this way and both failed; that
is what makes the third worth writing.

## Why

[The transducer ceiling](PLAYABLE-TONEBAR.md) settled which half of the
instrument the missing content lives in. A freely shaped transduction curve
reaches 98.03 dB² where the second motion coordinate reached 226.77 and the
tonebar 226.34, against 181.86 for the illegal geometry that ships. The
resonator has no headroom and the transducer has about 46%.

The model's pole is a disc: a sixteen-node quadrature over a circular face of
`pickup_pole_radius_m`. Muenster and Pfeifle describe the real one
differently — a round ferrite magnet whose end is **wedge shaped, pointing at
the tine**. A wedge ground to an edge is not a disc, and the difference is
not cosmetic: spreading the source across the direction the tine travels is
what produces the flux slope's sign inversion 390 µm out, which
[the geometry ceiling](PICKUP-GEOMETRY-CEILING.md) found sitting inside the
bass notes' swing. Collapsing that spread removes the inversion by
construction.

The ceiling experiment also measured what removing it is worth on its own:
resampling the shipping curve coarsely enough to smooth the inversion away
scored 155.82 against the sharp curve's 181.86, a 14% gain from smoothing
alone.

## The prediction

**Primary.** With a wedge pole, a fit confined to the service manual's gaps —
1.588 mm and wider in the bass — must reach **at or below 181.86 dB²** on the
nine training notes. Same bar, same cases, the one the last two failed at
226.77 and 226.34.

**Secondary.** The flux slope must stop changing sign anywhere inside the
swing the keyboard reaches, at every gap the manual allows and at the 0.5 mm
the model currently needs. This one is structural rather than statistical: it
either inverts or it does not.

**What is not predicted.** The wedge is not expected to reach 98.03. That
bound came from a curve that changes sign five times across the swing, which
no magnet produces; a realisable pole lands somewhere above it.

## What is being built

One continuous parameter, not a switch. The disc's nodes sit at
`(r cos t, r sin t)`; the wedge squashes them along the direction the tine
travels, to `(r cos t (1 - wedge), r sin t)`. At zero it is the disc every
profile has used, exactly. At one the source is a line across the tine's
path, which is an edge. In between it is the family a grinding wheel would
actually produce, and the fit can say where in it the instrument sits.

The orientation is an assumption and should be read as one: the sources say
the tip is wedge shaped and pointing at the tine, and do not say which way
the edge runs. Running it across the travel is the orientation that removes
the inversion, and that is the reason it was chosen — which is exactly the
kind of reasoning the prediction above exists to check rather than trust.

## The result

**The primary prediction is met, by a wide margin, and the candidate still
must not be promoted.** Both things are true and the second is the one that
decides what ships.

| Split | Shipping | Wedge candidate | Change |
| --- | ---: | ---: | ---: |
| Training, 9 notes | 181.86 | **73.07** | **−59.8%** |
| Held out, 6 notes | 373.95 | **205.47** | **−45.1%** |

That is the first thing in this work that has moved the number at all. The
second motion coordinate reached 226.77 and the tonebar 226.34 against a bar
of 181.86; the wedge reaches 73.07, and holds −45% on notes no fit here has
touched. The search drove the grind to **1.0**, its maximum: it wants a pure
edge, not a partly-ground face. It also turned the tonebar back **on** at a
coupling of 0.56, which nothing had done before — with a transducer that is
not fighting itself, the second prong earns a place.

**The secondary prediction is met, after being corrected.** A wedge crosses
zero exactly once across the swing, where the tine sits dead in front of the
pole; the disc crosses twice, adding the artefact at 390 µm.
`a_wedge_ground_pole_never_turns_the_flux_slope_over` holds that at four gaps
and three offsets. As written above the prediction asked for *no* sign
change, which was wrong: every pickup's flux slope passes through zero at the
pole's axis because the flux is at its extremum there. The prediction is left
as it was written and corrected here rather than quietly restated.

### Why it does not pass

The frozen promotion rule is not an aggregate rule alone. It also forbids any
reserved note regressing by more than ten percent, and two do:

| Note | Shipping | Wedge | Change |
| --- | ---: | ---: | ---: |
| 36 | 588.14 | 126.61 | −78.5% |
| **43** | 182.98 | 210.09 | **+14.8%** |
| 64 | 74.03 | 31.31 | −57.7% |
| 72 | 601.10 | 338.97 | −43.6% |
| **84** | 192.68 | 308.58 | **+60.2%** |
| 96 | 835.59 | 229.11 | −72.6% |

So the shipping voicing is unchanged and the wedge is off in every profile.
A candidate that improves the average by 45% while making one note 60% worse
is what the per-note rule exists to stop, and the rule is older than this
result.

### Two corrections to earlier claims

[The transducer ceiling](PLAYABLE-TONEBAR.md) said a freely shaped curve
reaching 98.03 "bounds what every possible pole can reach". It does not: it
bounds *transducer-only* variation at fixed mechanics, because that probe
held the shipping hammer law and geometry and moved nothing else. This fit
moves the mechanics too and reaches 73.07, below the supposed bound. The
number was right; the word "bounds" was too strong.

And the reserved notes are weaker than the protocol intends. No fit in this
work used them, but this session measured the error map across all 73 notes,
so they have been seen in aggregate. They are still the best available check
and they should not be read as pristine.

## What the next step needs

Not more tuning against these six. The search was three coarse starts that
finished at 73.05, 92.63 and 141.06, which is nowhere near converged, and a
better-converged fit on the training notes alone may not carry the two
regressions at all. That work is legitimate; reading these six while doing it
is not.

So: converge the fit on training notes, reserve notes that nothing in this
session has looked at, and evaluate once. Until then the wedge is a
measured, promising, unpromoted candidate, and `two_plane_fit` plus
`wedge_held_out` reproduce every number above.
