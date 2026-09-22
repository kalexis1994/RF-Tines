# Instrument presets: what each one stands for, and what it cannot

Date: 2026-09-21. Eight factory presets, each anchored to a documented change
to the real instrument, replacing five that were a smooth ramp with the word
"inspired" in their descriptions. The retired five stay loadable so saved
sessions keep working; they are no longer advertised.

## Why the old five had to go

They differed from one another mainly by hammer-tip hardness and bell, which
is the right *story* — tips and tines are two of the three things the sources
name as having the greatest impact on Rhodes tone. The trouble is that the
spans they used do not move this model.

Each control walked across the range those five presets covered, measured as
RMS spectral distance at G3 after matching level, so a volume change does not
read as a timbre change:

| Control | Span the old presets used | Attack | Body |
| --- | --- | ---: | ---: |
| **hardness** | 0.42 → 0.61 | **0.59 dB** | **0.03 dB** |
| bell | 0.22 → 0.46 | 4.11 | 2.33 |
| distance | 0.58 → 0.85 | 7.76 | 2.95 |
| alignment | 0.40 → 0.60 | 6.22 | 3.39 |
| dynamics | 0.30 → 0.70 | 7.67 | 2.57 |

So the era differences the descriptions promised were carried by the one
control that barely moved, while what the ear actually heard was the pickup
geometry, which the descriptions never mentioned.

`hardness` is not broken — over its whole range it is worth 7.02 dB in the
attack. Its mapping is just compressed in the middle: 0.42 to 0.61 is a 3.4×
change in contact stiffness, and contact time goes as the cube root of that,
so the hammer is on the tine for about 1.5× as long at one end as the other.
This agrees with the contact study in
[the pickup geometry ceiling](PICKUP-GEOMETRY-CEILING.md), where sweeping the
same stiffness over four decades moved the attack ratio by 0.04 dB. The new
presets therefore use hardness across 0.22 to 0.66 rather than 0.42 to 0.61.

## What the sources say changed

| Year | Change | Reported effect |
| --- | --- | --- |
| to early 1971 | Felt hammer tips | "Darker and mellower in attack than the later pianos" |
| 1960s | Raymac tines | "A bark that's entirely different from the later 1970's models" |
| early 1971 | Felt replaced by cube neoprene tips | The warm Mark I bark |
| mid 1975 | Wood/plastic hybrid hammer becomes fully plastic | "Tighter response" against the earlier "bounce" |
| early 1976 | Cube tips become graduated; Torrington tines become Singer | "The end of the early Fender Rhodes sound" |
| 1976–1982 | — | "Classic Rhodes bark begins to transform into the more bell-like tones" |
| 1977 | Peterson preamp replaced by Janus in the Suitcase | Different EQ, 5-pin connection |
| 1979 (Mark II) | Cosmetic at introduction, but the accumulated changes | "More bell like and somewhat less 'deep' and barky" |
| 1984 (Mark V) | Redesigned hammers | "Increased dynamic play in each note" |

The Suitcase has an active preamp with bass and treble and the stereo vibrato
that swings the output between two speakers; the Stage is passive, with a bass
control that is flat at full clockwise and can only cut.

## The presets

### Naming

**No preset is named after anybody's product.** This project already gave up
one name over exactly this — RF-Rhodes became RF-73 and then RF-Tines, and
[the rename](RENAMING.md) records the deliberate compatibility breaks it cost
— and a preset label sits in the host's browser with the same weight the
plugin's own name does. The visible names say what the voicing is and when
the change behind it happened. A year is a fact, not a model number.

The instruments, tine suppliers and preamps these voicings were derived from
are named in this document and nowhere else in the product, because citing
what a source says is the point of a document and is not the point of a
preset label.

| Preset | Derived from | Carried by |
| --- | --- | --- |
| Felt Tips 1966 | Felt tips and Raymac tines, before neoprene | Softest contact, least bell, pickup furthest out |
| Portable Bark 1972 | Cube neoprene, the warm Mark I bark | Mid contact, close well-aligned pickup, passive |
| Console Bark 1973 | The same piano through the Peterson preamp | Identical mechanism; preamp, bass lift, stereo vibrato |
| Portable Bell 1977 | Graduated tips, Singer tines | Firmer contact, more bell, pickup further out |
| Console Bell 1978 | Late Mark I through the Janus preamp | Same mechanism as 1977; brighter EQ, faster vibrato |
| Portable Chime 1980 | The Mark II's more bell, less deep bark | Most bell, pickup furthest of the late group |
| Wide Dynamics 1984 | The Mark V's redesigned hammers | Raised dynamics, the control that carries velocity reach |
| Tine Bass 1960 | The 32-note Piano Bass, E1 to B3 | Felt-era voicing, bass-weighted output |

Measured the same way, on physics alone, the eight separate by 2.4 to 5.9 dB.
Three pairs are closer, and two of those are meant to be: a Stage and the
Suitcase of the same years are the same piano, and what separates them is the
amplification, which this measurement excludes.
`a_suitcase_does_not_sound_like_the_stage_it_shares_a_mechanism_with` holds
both pairs to a stereo spread and an audible difference in the summed output,
because the preamp is the only thing keeping them apart.

The third close pair is Felt Tips 1966 and Tine Bass 1960, 1.47 dB apart.
That is correct rather than a defect: no source says the short bass keyboard's
tine or pickup voicing differed from the pianos of its era. It was the same
generator in a smaller box, and what makes it a different instrument is its
32-note range and its cabinet, not its tine.

## What these presets cannot express

**The hammer itself.** The 1975 change from a wood and plastic hybrid to a
fully plastic hammer is reported as the difference between a tight response
and a bounce, and that is hammer mass, not tip stiffness. `Profile` carries
`hammer_mass_kg`, but `Settings` does not expose it, so no preset moves it and
no preset claims the change.

**The tines as such.** Raymac, Torrington and Singer tines are named as a
major cause of the tonal shift. This model has no tine alloy or taper
parameter; the presets express the reported *result* — where the balance sits
between bark and bell — through the controls that reach it, not the cause.

**The geometry as measurement.** The pickup distances here run from 0.60 to
0.88 mm, and the service manual's normal minimum is 1.588 mm. Every voicing in
this plugin lives below what the factory permits, for reasons set out in
[the pickup geometry ceiling](PICKUP-GEOMETRY-CEILING.md): the model cannot
otherwise make the upper-partial content a real Rhodes has. These numbers are
model parameters, not recovered dimensions, and no measurement of a real
instrument should be read out of them.

**Any claim of being the instrument.** Each description ends with "editable
approximation, not an exact replica" and means it. None of these was fitted to
a recording of the instrument it names; the only reference this project holds
is one Mark I, and the presets other than the fitted `calibrated` entries were
set by mapping documented changes onto controls that measurably carry them.

## Sources

| Source | Used for |
| --- | --- |
| [Chicago Electric Piano, Rhodes timeline](https://chicagoelectricpiano.com/blogs/news/the-ultimate-fender-rhodes-timeline) | Year-by-year hammer, tip and tine changes and their reported tonal effect |
| [Chicago Electric Piano, Mark I vs Mark II](https://chicagoelectricpiano.com/rhodes/fender-rhodes-mark-i-vs-rhodes-mark-ii/) | Which three factors most affect tone; the Mark II's character |
| [Vintage Vibe, guide to hammer tips](https://www.vintagevibe.com/blogs/news/the-definitive-guide-to-vintage-vibe-hammer-tips) | Felt to neoprene in early 1971; tip colour coding by density and register |
| [Rhodes Service Manual, chapter 4](https://www.fenderrhodes.com/org/manual/ch4.html) | Pickup-to-tine gap standards and the timbre adjustment |
| [Rhodes piano, Wikipedia](https://en.wikipedia.org/wiki/Rhodes_piano) | Model list and production years; Piano Bass range; Suitcase and Stage configurations |
