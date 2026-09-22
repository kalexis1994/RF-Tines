# Four faults a listener found that the objective cannot see

Date: 2026-09-22. Over one session a listener identified four distinct
faults by ear, without seeing any measurement. Every one turned out to be
real, reproducible and quantified. **Not one of them is visible to the
calibration objective.**

That is the finding. The individual faults matter less than what they say
together about how this model is being judged.

## The four

| What was heard | What it is | Where the model stands |
| --- | --- | --- |
| "on recordings the attack is softer" | Upper partials over the fundamental, in the attack | +9.2 dB too much at G3 soft; the circuit law brings it to +1.2 |
| "a short tick the real one doesn't have" | 5–10 kHz in the first 12 ms | **+35 to +60 dB too much, at every note and layer** |
| "a presence hit, more inflated" | 0.7–3.5 kHz in the first 15 ms | −11 dB at G3 forte, and the real's swell is velocity-dependent where ours is flat |
| "more space, not so compressed" | How far the envelope wanders from a straight line | 0.06 dB against the real's 0.45 at G3 soft: ours is very nearly a perfect exponential |

`listening_axes` measures all four against the reference for any profile.
It asserts nothing, because three of the four are deficits the model still
has and a test can only pin what is already true. It exists so the next
candidate is judged on the axes a listener used.

Running it turned up a fifth the ear had not separated: at E3 the model is
**48 dB short** of the reference's upper-partial content, soft and loud
alike. The upper register is emptier than anything else measured here.

## What the objective sees, and what it does not

The frozen objective scores the second, third and fourth harmonics relative
to the fundamental, in a 96 ms attack window and a body window. It therefore
has no term for:

- anything above the fourth harmonic, which is where the tick lives;
- anything in the **first 15 ms**, because its attack window is 96;
- the **shape of the envelope**, only its content;
- and, as an earlier candidate proved, a **second pitch** beside the
  fundamental — one scored 73.07 and was rejected on hearing because it rang
  78 cents away at −10.7 dB.

Four fits in this project drove parameters to their bounds chasing that
objective. It is a reasonable objective and it is not a definition of the
instrument, and the gap between the two is what a listener kept finding.

## The most valuable thing found all day was already there

The tick is the hammer contact. Traced, the hammer is on the tine for
**116 µs**; the piano literature puts hammer contact between 0.3 and 4 ms.
Softening `contact_stiffness` from 4e10 to 4e8 brings contact to 547 µs,
takes 24 dB off the tick, and **improves the objective by 41%** on the
training notes — a single parameter that already existed, no new physics.

The reason it took a day to find is on record and is worth keeping there.
[The pickup geometry ceiling](PICKUP-GEOMETRY-CEILING.md) swept the same
parameter across four decades in the first hour and called it inert. That
was measured on the attack-over-body envelope, which is deaf to a
high-frequency burst lasting twelve milliseconds. The parameter was never
inert; the axis was wrong, and that measurement sent the whole day after
missing physics instead.

Note also that the `hardness` control cannot reach the better value: its
softest setting is 1.6e9, four times stiffer than the 4e8 optimum. Fixing
this properly means reparameterising the mapping, not moving a default.

## What this reopens

[The tonebar](PLAYABLE-TONEBAR.md) concluded that at the prong spacing the
measurements report, the two prongs are too far apart to trade energy, so
the envelope stays straight — and recorded that as a failed prediction.

**The reference's envelope was never measured.** It is not straight: the real
instrument leaves 0.45 dB against a straight line at G3 soft where a lone
tine leaves 0.06. The real fork does something the model does not. That does
not make the coupling implemented there correct, but it does mean the
mechanism was dismissed on a comparison that was never made.

Twice in one session something was called inert without checking the right
axis or the reference. Both times a listener found it anyway.

## What should happen before more physics

1. **Soften the contact** and reparameterise `hardness` around a contact
   duration the literature supports. Largest measured gain available, and it
   removes an audible artefact.
2. **Give the objective terms it lacks** — early attack, envelope shape, and
   the one-pitch guard that already exists as a test. Until then every fit is
   free to buy score with things a listener rejects.
3. **A fresh reserve.** The six reserved notes were spent this session, on
   fitting and then on diagnosis.
4. Only then the presence swell, the empty upper register, and whatever the
   fork really contributes.
