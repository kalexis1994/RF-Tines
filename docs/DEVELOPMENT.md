# Development

## Toolchain and dependency

Rust 1.98.0 is pinned. The DSP has no third-party dependencies. Offline analysis uses `hound` for WAV decoding and `serde`/`serde_json` for reports; its FFT is implemented and tested in Rust. The plugin also uses serde JSON for control-side declarative editor documents, but never in rendering or parameter automation. Cargo.lock records exact versions. The plugin uses the public Rust SDK and `rackforge-program-api` from a sibling `rackforge` checkout through explicit Cargo paths. For this prototype use RackForge revision `7c17bd4a480d1c0bd7fa18fa4d880e82429dffe1`. A local path dependency is not a reproducible distribution pin: before external releases, replace it with an exact published version or Git revision and regenerate Cargo.lock.

On this Windows GNU setup, put `C:/msys64/ucrt64/bin` on the current shell's PATH so Rust can find the linker and dlltool. No machine-wide environment changes are needed.

```powershell
$env:Path = 'C:/msys64/ucrt64/bin;' + $env:Path
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
cargo build --locked --release --workspace
cargo build --locked --release --target wasm32-unknown-unknown -p rf-tines-plugin
```

The PowerShell line only configures the shell; every build tool, renderer and test in the project is Rust.

Version 0.1.2 adds the `rf-tines-ui` Rust WebAssembly PLAY panel. Install the matching generator with `cargo install wasm-bindgen-cli --version 0.2.127 --locked`. `cargo run --locked --release -p rf-tines-lab -- build-ui` builds it and generates the browser bindings. Packaging and audition invoke this step automatically. Run `cargo clippy --locked -p rf-tines-ui --target wasm32-unknown-unknown -- -D warnings` as well as native workspace Clippy, since browser code is target-gated. HTML/CSS are static assets; generated JavaScript contains the browser ABI glue and startup call, while UI behavior remains Rust.

When disk space is limited, set `$env:CARGO_INCREMENTAL = '0'` in the build shell to avoid regenerating incremental compilation caches. This changes only that shell's builds and may increase rebuild time; normal dependencies and release artifacts still occupy space in `target`.

## Render a MIDI file

```text
cargo run --locked --release -p rf-tines-lab -- render-midi SONG.mid --output renders/song.wav --normalize
cargo run --locked --release -p rf-tines-lab -- render-midi SONG.mid --output renders/song-lab.wav --normalize --pickup 3 --sustain calibrated
```

Format 0/1 files with their tempo map are rendered through the playable 0.1.2
engine at 48 kHz: every non-drum channel is merged, note on/off, sustain
(CC 64) and all-notes-off are honoured, and notes outside the 73-key range are
counted and dropped. The raw WAV is unnormalized float; `--normalize` writes a
separate `-norm.wav` peaking at -1 dBFS. `--pickup 0..3` renders the
laboratory engine's level-matched pickup path instead of the raw engine; path
3 is [Close Aperture](PICKUP-APERTURE-PATH.md). `--sustain calibrated` uses the
[recording-derived per-partial T60](PLAYABLE-SUSTAIN.md); `--bar-ratio`,
`--bar-strike` and `--contact-stiffness` set the [second partial's](PLAYABLE-BAR-PARTIAL.md)
ratio and strike weight and the contact coefficient; `--law`, `--pole-radius-mm`
and `--velocity-exponent` set the [pickup law and velocity curve](VOICING-PHYSICS.md);
every render reports its level compensation and `--compensate` applies it. `--gain`,
`--sample-rate` and `--tail` are documented in `--help`. The JSON receipt records note counts, peak, RMS,
faults and the render speed. The engine is uncalibrated and has no limiter.

## Render and inspect

```text
cargo run --release -p rf-tines-lab -- render --output renders/a3.wav --trace
cargo run --release -p rf-tines-lab -- demo --output renders/demo.wav
cargo run --release -p rf-tines-lab -- inspect renders/demo.wav
cargo run --release -p rf-tines-lab -- stress
```

`--help` lists validated render parameters. Output files use create-new semantics; choose a new name to rerun. A failed disk write may leave a partial file; `inspect` checks the laboratory's WAV structure, exact length and finite samples. An existing report or trace also prevents accidental overwrite.

The raw WAV has no automatic normalization or clipping. Its JSON report exposes peak and RMS. Output gain is intentionally conservative for ordinary notes, but dense stress chords can exceed full scale. Use host gain when auditioning.

## Analyze recordings

```text
cargo run --locked --release -p rf-tines-lab -- analyze renders/a3.wav --output renders/a3-analysis.json --note 57 --sustain-end 1.8
cargo run --locked --release -p rf-tines-lab -- analyze references/audio/a3.wav --output renders/reference-analysis.json --channel 0 --note 57 --sustain-end 3
cargo run --locked --release -p rf-tines-lab -- analyze references/audio/a3.wav --output renders/reference-long.json --channel 0 --note 57 --sustain-end 5 --partial-window-ms 1024
cargo run --locked --release -p rf-tines-lab -- compare references/audio/a3.wav renders/a3.wav --reference-channel 0 --output renders/comparison.json
cargo run --locked --release -p rf-tines-lab -- compare-partials references/audio/a3.wav renders/a3.wav --reference-channel 0 --output renders/partial-comparison.json --seconds 2 --reference-start 0.2 --candidate-start 0.2
```

Use `analyze` to read external WAV files; `inspect` remains the strict checker for the renderer's own WAV format. Comparison requires matching sample rates. See [Analysis laboratory](ANALYSIS.md) before interpreting metrics or choosing a sustain boundary. No reference audio is included in this repository.

## Attack and body tone comparison

For short attack/body comparisons without a declared sustain boundary or velocity mapping:

```text
cargo run --locked --release -p rf-tines-lab -- compare-tone references/audio/jrhodes-g3-a886e6c/A_055__G3_1.wav renders/g3-baseline.wav --note 55 --output renders/g3-tone.json
```

Generate the candidate WAV first using [G3 residual pilot](G3-RESIDUAL-PILOT.md). [Tone comparison](TONE-COMPARISON.md) specifies the fixed observation windows and relative harmonic metrics.

## Pickup geometry sweep

To generate three level-matched listening tracks and observe isolated/polyphonic headroom:

```text
cargo run --locked --release -p rf-tines-lab -- pickup-listening --output renders/pickup-listening
```

See [Pickup listening](PICKUP-LISTENING.md) for the exact performance, constant-gain matching and output names. Use `--measure-only` to write the complete receipt without WAVs.

For a register-specific convergence check of both laws, including an independently refined reference:

```text
cargo run --locked --release -p rf-tines-lab -- converge-pickup --output renders/treble-pickup-convergence.json --note 100 --velocity 0.2 --sample-rate 48000 --gap-mm 0.5 --offset-mm 0.25 --reference-steps 256
```

See [Pickup convergence](PICKUP-CONVERGENCE.md) for the frozen-trajectory diagnostic, physical filter matching and numerical interpretation.

To compare transfer laws under identical production mechanics and filtering:

```text
cargo run --locked --release -p rf-tines-lab -- render-pickup-pair --output renders/g3-pair.wav --note 55 --velocity 0.9 --sample-rate 44100 --seconds 3 --hold 2.8
```

See [Mechanical pickup pairs](PICKUP-PAIR.md) for the companion WAV/report names, duration constraints and reference experiment. Version 0.1.2 also exposes the alternative in the real-time [Pickup Lab UI](PICKUP-LAB-UI.md).

For a bounded pickup-geometry experiment against an explicitly selected held-note region:

```text
cargo run --locked --release -p rf-tines-lab -- sweep-pickup references/audio/a3.wav --output renders/pickup-sweep.json --note 57 --velocity 0.7 --seconds 1 --reference-start 0.1 --model-start 0.1 --gaps-mm 1,1.5,2 --offsets-mm 0.25,0.5,0.75
```

See [Pickup sweep](PICKUP-SWEEP.md) for input bounds, ranking semantics, reference requirements and a reproducible synthetic recovery example. Candidate audio stays in memory; the only output is a create-new JSON report.

## Shared pickup fit and held-out validation

To fit several recordings and evaluate reserved notes/intensities with frozen geometry and gain:

```text
cargo run --locked --release -p rf-tines-lab -- fit-pickup-set references/pickup-set.synthetic.json --output renders/pickup-set-demo/result.json
```

First generate the example's three reference WAVs using [Pickup reference set](PICKUP-SET.md). That document also specifies provenance, shared capture gain, held-note regions and the strict JSON manifest contract.

## Contact refinement experiment

```text
cargo run --locked --release -p rf-tines-lab -- converge --output renders/treble-convergence.json --note 100 --velocity 0.2
```

The command compares fixed 4/8/16/32x integration and the production contact-refined voice against a finite 64x reference. It uses a common offline filter and writes mechanical, attack and full-window errors without automatic alignment. See [Numerical convergence](CONVERGENCE.md). Duration is limited to 0.05–1 second and velocity to 0.01–1 so the experiment stays bounded and above negligible excitation.

## The lock file belongs to the pinned host, not to yours

`Cargo.lock` records the version of every path dependency, and the RackForge
crates are path dependencies. CI checks the host out at the commit
`.github/workflows/*.yml` pins and builds with `--locked`, so the committed
lock has to carry *that* host's version. A sibling checkout of RackForge that
has moved on will differ.

This is a trap, because it fails quietly in the direction that matters.
Running `cargo update` against a newer sibling rewrites the lock to the newer
version, every local `--locked` command keeps passing, and CI fails with
"cannot update the lock file ... because --locked was passed". It happened
once: the lock went from 0.1.20 to 0.1.23 while fixing an unrelated version
skew, and the push failed on all three CI jobs.

To regenerate the lock, point the two path dependencies at a worktree of the
pinned commit, run `cargo update -w --offline`, then restore the paths. Only
`Cargo.lock` should change. The consequence is that `--locked` will then fail
against a newer local host, which is correct: the pinned host is what is
supported, and that failure is the check working rather than a problem to fix
by updating the lock.

## Package

For the complete build/install/launch cycle, use `cargo run --locked --release -p rf-tines-lab -- audition`. See [Desktop audition](AUDITION.md) for the dedicated library, settings retention and repeat-run behavior. The standalone `package` command below remains useful for producing a versioned release archive without launching a host.

The research manifest uses supported legacy schema 1 and RackForge's generic appearance. No HTML, JavaScript or custom GUI is included. Gain and the Research Direct program are exposed through host contracts.

Build RackForge's current Rust `rackforge-store` and `rackforge-core` tools in its own repository (`cargo build --locked --release -p rackforge-store -p rackforge-core`). Then from RF-Tines run:

```text
cargo run --release -p rf-tines-lab -- package
```

The Rust laboratory builds the component itself, resolves the sibling host
tools, copies the WASM to ignored `package/component.wasm`, validates
metadata, renders through the host and creates the archive only after
validation succeeds. It used to check only that a component existed, so an
artifact left over from another version packaged silently and the only tell
was the smoke test's peak moving in the fourth decimal. Building is not
enough on its own -- cargo considers an artifact up to date by fingerprint and
will not replace one that was tampered with or restored from elsewhere -- so
the component is also checked for this crate's version string, which reaches
it through the program descriptor's `plugin_version`. It never overwrites an existing archive. Old prebuilt host binaries may not support the current API; rebuild them from the pinned source. To inspect or smoke-test manually:

```text
../rackforge/target/release/rackforge-core inspect package
../rackforge/target/release/rackforge-core smoke package --preset research-direct --data-root dist/smoke-data
```

The package is a research build, with one immutable physical profile and one user parameter. A dedicated instrument UI, branded schema 3 package, profile controls and calibrated presets are later milestones.

## Repository layout

```text
crates/rf-tines-dsp/       model, contact solver, voices, pickup and decimation
crates/rf-tines-analysis/  offline audio decoding, spectra, envelopes and comparison
crates/rf-tines-plugin/    SDK adapter, MIDI validation and versioned state
tools/rf-tines-lab/        Rust rendering, traces, reports and stress measurements
package/                   RackForge manifest and metadata
docs/                      English design, measurement and development documents
renders/                   ignored generated WAV, CSV and JSON
dist/                      ignored distributable and validation outputs
```

The DSP and laboratory forbid unsafe Rust. The plugin export macro contains the SDK's raw ABI implementation; handwritten adapter code uses safe Rust. All event lists are validated before mutation, and invalid blocks are silenced without applying partial edits.

## Local cache budget

Set `CARGO_INCREMENTAL=0` for local verification and reuse this workspace's
existing target directories. Inspect their size before creating large render
matrices. Use compact numeric receipts when an additional WAV is unnecessary.
After Cargo jobs have finished, `cargo clean --profile dev` removes regenerable
debug artifacts while retaining the release tool. Verify the configured target
directory belongs to this workspace before cleanup; do not clean a shared or
redirected target directory implicitly. The next debug test run will rebuild.
Preserve source recordings, tracked receipts and user-facing test WAVs.
