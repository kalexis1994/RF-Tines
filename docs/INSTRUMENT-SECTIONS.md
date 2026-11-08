# RF-Tines instrument sections

## Reference: RF Concert Grand

Reviewed RackForge's `plugins/concert-grand`: the PLAY surface, parameter
metadata and model controls in `src/lib.rs`. Concert Grand separates everyday
voicing from Room & Microphones, Action & Noises, laboratory controls and
individual model subsystems. RF-Tines adopts the subsystem organization while
keeping its controls tied to the implemented electric-piano model.

The relevant RF-Tines signal path is:

```text
MIDI touch -> hammer contact -> tine / tonebar motion -> magnetic pickup -> output
                              ^
                         key / damper / pedal
```

## First implemented structure

The same four sections are used by the PLAY surface, RackForge parameter
metadata and the native preset editor. Existing parameter indices, field IDs,
values, preset IDs and saved state are unchanged.

| Section | Existing controls | Meaning and limits |
| --- | --- | --- |
| Hammer & Touch | Hammer Hardness; Dynamics | Contact stiffness and MIDI-velocity-to-hammer-speed mapping. Dynamics is a performance mapping, not a separate physical component. |
| Tine & Tonebar | Sustain; Bell | Sustain currently couples tine and tonebar decay. Bell changes excitation of the second bending mode; it is not a bell oscillator or a proven one-to-one tonebar adjustment. |
| Pickup | Pickup Distance; Tine Alignment; Pickup Model | Base gap, lateral offset and transfer model. Register Aperture includes the frozen upper-register geometry. |
| Output | Output Gain | Gain after shared pickup compensation. Stereo-linked peak protection follows the panel electronics. |

The PLAY page uses four accessible tabs with one visible panel, starting in
Hammer & Touch. Switching panels does not navigate to a fragment or scroll
the page. Left/Right arrows, Home and End move between tabs; Tab enters the
visible controls. The native editor presents the same sections as pages. The pickup
model choice remains available but is placed after the physical position
controls. Original preset/state behavior is preserved.

## Next controls, ordered by physical usefulness

1. **Separate resonator losses.** Split tine sustain from upper-mode decay;
   migrate the existing coupled Sustain parameter explicitly, keeping old
   presets identical. Do not silently change what its automation means.
2. **Dampers & Pedal.** Expose validated release damping and contact behavior.
   The current keyboard/pedal mechanism already operates, but there is no
   editable release control or validated continuous half-pedal control in the
   playable preset. Add a section only with working controls.
3. **Hammer voicing.** Consider strike speed/range and effective contact
   parameters after checking response and output across velocities. Hardness
   alone is not all hammer-tip material behavior.
4. **Instrument setup.** Tuning and bounded register voicing belong here once
   they are implemented and persist independently of the preset model choice.
   Per-key tuning is not currently exposed by the eight-parameter interface.
5. **Amplification & Effects.** A future electrical signal chain can contain
   preamp, tremolo and cabinet. Keep it distinct from tine/pickup physics and
   make bypass explicit. These blocks are not implemented by this refactor.

Only add an advanced Model section when useful controls exceed the musical
pages. Raw solver constants, numerical quadrature settings and fit objectives
belong in research tools, not in the instrument's playing interface.

## Why not copy the piano pages directly?

Concert Grand's strings, unison, soundboard and microphone controls correspond
to its model. RF-Tines currently models a struck resonator read by a magnetic
pickup; the matching user concepts are hammer contact, resonator behavior,
pickup placement and electrical output. A microphone or room section would
require an actual amplifier/acoustic output stage rather than a renamed DSP
constant.

This change reorganizes existing controls and clarifies their descriptions.
It introduces no DSP change and does not claim a new audible improvement.

Version 0.1.5 gives PLAY a brushed-metal nameplate, dark control panel,
wood-tone side rails and metal faders. Both host lighting modes and narrow
layouts remain supported. Tab behavior is implemented in Rust alongside the
existing host protocol; no DSP or state format change is introduced.


## 0.1.6 program navigation and rotary controls

The PLAY surface reads the catalog and selected identity from host context,
then selects through `plugin.select_sound`. Parameter writes finish before a
selection; remaining edits are discarded, and selection always triggers a fresh
parameter snapshot. A host-side program change invalidates older snapshots.
Failed or timed-out selection is not automatically replayed; the error remains
visible while current parameters are recovered.

At 960 px of iframe width, programs occupy the left column. Narrower layouts
use a native program selector with previous/next buttons. This avoids a second
modal focus system and preserves native touch and keyboard behavior. Existing
factory programs are retained; musical banks and new electronic controls remain
next steps from `PLAYER-PANEL-AND-PROGRAMS-RESEARCH.md`.

Continuous controls are rotary dials backed by native ranges. Vertical pointer
drag, Shift fine adjustment, keyboard arrows, editable numeric values and a
factory-default double-click reset are supported. Program selection and host
parameter updates also update the dials. No DSP or saved-state layout changed.


## 0.1.7 instrument panels and era-inspired programs

The opening Instrument page selects Stage (Volume, Bass Boost) or Suitcase
(Volume, Bass, Treble, Vibrato On/Off, Speed, Intensity). Hammer, Resonator and
Pickup expose the internal model; Setup holds the keyboard response. These
are simplified electronics inspired by player controls, not circuit emulations.

Seven appended parameter IDs (8-14) leave the original IDs untouched. State
version 5 appends 56 bytes to the version-4 prefix; versions 1-4 still load.
Version-4 custom-program payloads default missing electronics to a neutral
Suitcase path. New state is 124 bytes. Transfer capacity is now 16384 bytes to
accommodate the expanded editor and ten factory plus eight custom programs.

Low/high tone shaping uses first-order complementary shelves at 200/2500 Hz,
with -12..12 dB nominal gain settings. Stage Bass Boost blends a 200 Hz high-pass
response toward flat: fully clockwise restores bass. Frequencies, tapers and
ranges are RF-Tines design choices, not historical measurements. Tone gains,
mode, modulation depth and speed settle with 10 ms exponential smoothing.

Suitcase Vibrato is sinusoidal, complementary amplitude modulation. Each
channel gain stays between zero and one. A single-channel host receives the
left-channel tremolo; averaging stereo cancels the movement and retains the
mean attenuation. Bypassed modulation has unity gain. The final output protects
both channels with the same gain above 0.95 FS and recovers with a 50 ms time
constant. Electronic edits do not rebuild the physical voice profiles.

### Instrument programs

All five programs use Register Aperture and the same Dynamics (0.5). Values
are original design approximations within our model, not measured historical
specifications. Names indicate inspiration; individual vintage instruments
vary with setup, wear, service and amplification. No listening validation
against separate period instruments has been completed for these programs.

| Program | Hardness | Sustain | Bell | Gap mm | Alignment mm | Electronics |
| --- | --- | --- | --- | --- | --- | --- |
| Stage 73 - Early '70s | .42 | .48 | .22 | .75 | .48 | Stage, Bass Boost .90 |
| Suitcase 73 - Mid '70s | .46 | .52 | .28 | .58 | .40 | Bass +1 dB, Treble -1 dB, 3.2 Hz / .55 depth |
| Stage 73 - Late '70s | .55 | .45 | .38 | .70 | .55 | Stage, Bass Boost .82 |
| Suitcase 73 - Late '70s | .57 | .46 | .40 | .62 | .52 | Bass -1 dB, Treble +1.5 dB, 4.6 Hz / .50 depth |
| Stage 73 - '80s | .61 | .40 | .46 | .85 | .60 | Stage, Bass Boost .78 |

The previous five IDs remain in the Reference bank without setting changes.
The new Instruments bank appears first. User programs remain host managed.

Historical control and family references:
- [Original service manual](https://www.fenderrhodes.com/service/manual.html):
  documents earlier and later tone sources/actions and Suitcase electronics.
- [Stage owner operation](https://www.fenderrhodes.com/img/service/guides/stage-mark2/p5.jpg).
- [Suitcase owner panel](https://www.fenderrhodes.com/img/service/guides/suitcase-mark1/p2.jpg).

Validation includes neutral bit identity, migrated-state audio equality,
invalid-state rejection, EQ polarity at three sample rates, modulation period
and complementary channels, event/block-size invariance, and dense repeated
10-note chords for each new program. Peak checks establish headroom only for
that test, not every possible performance or boosted-EQ setting.


## 0.1.8 factory catalog cleanup

The five research/reference programs and their Reference bank are no longer
advertised. The catalog contains only the five era-inspired instrument programs
and any user programs. Retired factory IDs remain loadable for compatibility
with stored sessions and program references. Their settings and the state format are unchanged.

Knob lighting and cast shadows stay fixed; only the position marker rotates.

Stage 73 - Early '70s is first in the catalog and initializes fresh processor
instances. Parameter defaults and UI resets match this voice. Legacy state
migration keeps its original neutral-electronics defaults.

## 0.1.10 portable program files

The CONFIG surface exports and imports `.rftines` files. Schema 1 wraps one
RackForge program document with a canonical SHA-256 checksum. Imports are
limited to 32 KiB, reject unknown fields, foreign plugin IDs, unsupported state
versions and out-of-range parameters, then pass through the plugin's existing
program validation before the host saves them atomically. Imported IDs are made
unique, so loading a file never silently overwrites a user program. Version-4
documents remain importable with neutral electronics defaults.

## 0.1.11 controller roles and density

The public parameter schema maps RackForge's standard plugin level and
expression roles to Volume, LFO rate to Speed, and both LFO depth and
performance modulation to Intensity. Sustain remains ordinary performance MIDI
CC64 because it operates the dampers rather than the physical Sustain voicing
parameter.

The PLAY and CONFIG surfaces hide their connection indicator after the bridge
is ready. Desktop controls, spacing and headings are denser, while layouts at
760 px and below retain larger rotary and numeric touch targets.

## 0.1.12 local program workflow

CONFIG separates library persistence from file transport. Save current controls
creates a named program in RF-Tines's private host storage. Export only downloads
an existing factory or user program, preserving its program name and identity.
Import validates a `.rftines` file and allocates a new local user-program identity,
so it never replaces an existing program. Portable names allow Unicode while
rejecting control and formatting characters.

## 0.1.13 save from PLAY

PLAY owns creation of local user programs because it is where controls are
edited and auditioned. `SAVE AS` opens a naming dialog, waits until the host
parameters match every displayed control, saves a new program in RF-Tines's
private library and selects it. CONFIG now focuses on exporting saved factory
or user programs and importing `.rftines` files as new local programs.

Pitch Bend remains a performance gesture instead of a saved semantic
parameter. MIDI 1.0's full 14-bit wheel and MIDI 2.0's 32-bit value now retune
ringing resonators over ±2 semitones. The gesture preserves each mode's current
motion and physical loss, applies to future notes on the same channel, and
returns to exact tuning at the wheel center.
