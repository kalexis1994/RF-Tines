# Changelog

All notable RF-Tines changes are recorded here. Versions follow semantic
versioning while the public plugin contract is still below 1.0. Entries for
releases published before the rename keep the names that were current then.

## 0.2.7 - 2026-09-22

- Gave the tonebar a path to the tine that does not depend on elastic mixing.
  `tonebar_coupling` was doing two physically separate jobs: the rigid clamp
  that bolts the tine to its bar, which is always there, and the elastic
  mixing that pulls the two resonances together, which beats. At zero the bar
  was, in the code's own words, "neither struck nor heard"; at the coupling
  that made it audible the note acquired a vibrato a listener described as
  sounding "como una cuerda". The new `tonebar_clamp` carries the first alone.
- **Limited the clamp by the bar's inertia.** A flat clamp cost the treble its
  level -- 14.7 dB at note 93 forte, 8.8 dB at note 84 -- because a heavy bar
  cannot follow a fast root motion, so it took the hammer's energy and
  dissipated it in its own short decay. The clamp is now scaled by
  `1 / (1 + (f / tonebar_clamp_reference_hz)^2)`, and the treble returns to
  its no-bar level.
- Measured that corner frequency against the reference instead of assuming it.
  It had been set to 220 Hz because the rest of the voice scales there; fitted
  on the training notes the optimum is broad and flat from 195 to 235 Hz with
  its minimum at **210 Hz**, which is what now ships. The assumption held, but
  it was not a measurement until now.
- Frozen objective 107.37 -> 97.29 on the training notes, a 9.4% improvement,
  with neither of the two new attack terms regressing. See
  [the tonebar](docs/PLAYABLE-TONEBAR.md) and
  [the attack protocol](docs/ATTACK-PROTOCOL.md).
- Corrected `state_version` in the package manifest from 5 to 6, matching the
  `STATE_VERSION` the plugin has actually written since the profile grew its
  second axis. The host records that number alongside every saved state and
  folds it into the state digest, so the two disagreeing meant saved states
  were labelled with a schema they were not written in. The plugin's own
  migration reads the version from the blob, so nothing was mis-loaded.
- Extended the calibration protocol to the attack, which three earlier
  decisions had been made without: the contact was softened, the tonebar was
  switched off and a resonance bank was discarded, all on an objective that
  scores H2-H4 over windows far longer than the fifteen milliseconds a
  listener calls the attack. Two terms are added beside it, reported
  separately and never summed, and the reserved note set is replaced because
  the old one had been seen in aggregate.

## 0.2.6 - 2026-09-22

- Audition build, superseded by 0.2.7. First version with the inertially
  limited clamp, at an assumed 220 Hz corner.

## 0.2.5 - 2026-09-22

- Audition build, superseded. Shipped the clamp flat at 0.6, which cost the
  treble up to 14.7 dB of level. The transient term had already reported a
  228% regression before it was packaged, and it was packaged anyway.

## 0.2.1 - 2026-09-18

- Fixed the instrument faceplate on the PLAY surface, which still read `RF–73`.
  The wordmark is split across three nodes as `RF<span>–</span>73`, so the
  literal string `RF-73` never occurs and every text search over the source, the
  published package and the RackForge release reported it clean. It now reads
  `RF–Tines`.
- Kept that wordmark on one line. The dash is a line-break opportunity and the
  new name is four characters longer, so a narrow panel could have split it.

## 0.2.0 - 2026-09-18

- Renamed the project from RF-73 to RF-Tines across crates, the laboratory tool,
  package metadata, both Web surfaces, CI and documentation. See
  [naming and compatibility](docs/RENAMING.md).
- **Breaking:** the plugin ID is now `org.rackforge.rftines` and the instance ID
  `desktop.org.rackforge.rftines`. Sessions, host presets and custom programs
  saved under the previous `org.rackforge.rhodes` identity no longer resolve, and
  a host that already holds 0.1.14 treats this as a different plugin.
- **Breaking:** portable programs are now `.rftines` files with format string
  `rackforge.rftines-program` and media type
  `application/vnd.rackforge.rftines+json`. Previously exported `.rf73` files are
  rejected, both by the format check and by the plugin-identity check.
- Changed the desktop audition override to `RF_TINES_DESKTOP` and the library
  ownership marker to `.rf-tines-owned`. An existing library marked
  `.rf-73-owned` or `.rf-rhodes-owned` is still adopted with its settings intact.
- Replaced the catalog icon, banner and loading splash with artwork carrying the
  RF-Tines wordmark.
- Unchanged: the state schema, parameter IDs, factory program IDs, the state
  byte layout and all DSP behaviour. The packaged smoke check reports the same
  peak and state size as 0.1.14.

## 0.1.14 - 2026-09-16

Published as RF-73, before the rename.

- Added the RF-73 icon, catalog banner and loading splash and moved the package
  to RackForge manifest schema 3.
- Added a responsive Stage/Suitcase front panel with five era-inspired factory
  programs and compact physical controls.
- Added local program saving from PLAY and `.rf73` import/export from CONFIG.
- Added channel-aware MIDI 1.0 and MIDI 2.0 pitch bend with a fixed two-semitone
  range that preserves the state of ringing physical modes.
- Added a reproducible package job to CI and a tag-gated GitHub release workflow
  that publishes the validated `.rfplugin` with its SHA-256 checksum.
