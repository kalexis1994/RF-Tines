# Sources and evidence scope

Research consulted on 2026-09-04. Implementation choices are distinguished from measured facts in MODEL.md.

See [September research review](RESEARCH-2026-09.md) for paper findings, open Rust projects, recording provenance, access limitations and the proposed implementation sequence.

| Primary source | Intended use | Material consulted |
| --- | --- | --- |
| [Rhodes Service Manual, 1979](https://www.fenderrhodes.com/service/manual.html) | Construction and variants | Index and foreword |
| [Chapter 1](https://www.fenderrhodes.com/org/manual/ch1.html) | Tine–tonebar assembly | Manual transcription |
| [Chapter 4](https://www.fenderrhodes.com/org/manual/ch4.html) | Escapement, strike line, timbre and level | Manual transcription |
| [Pfeifle, DAFx 2017](https://www.dafx.de/paper-archive/2017/papers/DAFx17_paper_79.pdf) | Contact, polarizations and pickup | PDF, especially sections 3–5 |
| [Muenster and Pfeifle, ISMA 2014](https://www2.conforg.fr/isma2014/cdrom/data/articles/000062.pdf) | Where the growl is made, and where it is not | PDF; high-speed camera and piezo measurements |
| [Falaize and Hélie, JSV 2017](https://www.sciencedirect.com/science/article/pii/S0022460X16306320) | Passive simulation and modal reduction | Publisher abstract and author demonstration page; full manuscript not inspected; DOI 10.1016/j.jsv.2016.11.008 |
| [Antoine Falaize demonstrations](https://afalaize.github.io/posts/rhodes/) | Pickup geometry and reproducible reference | Text and links; linked code/audio not evaluated |
| [Gabrielli et al., JASA 2020](https://iris.univpm.it/handle/11566/286030) | Inharmonic attack modes and intermodulation | Institutional abstract only; DOI 10.1121/10.0002002 |
| [Shear and Wright, NIME 2011](https://www.nime.org/proc/nime2011_shear/index.html) | Experiments on an augmented Rhodes | Proceedings record and abstract; related thesis measurement tables remain unverified |

Outstanding: select an actual reference instrument; acquire direct recordings; read the full JASA modal study; quantify tonebar/mount contributions; calibrate the contact and pickup models; evaluate higher-rate reference renders.

RackForge integration was inspected at revision `7c17bd4a480d1c0bd7fa18fa4d880e82429dffe1` (workspace 0.1.14). Relevant local sources are its public plugin SDK, `docs/PLUGIN_ABI.md`, `docs/PLUGIN_DEVELOPMENT.md`, and Concert Grand's model and laboratory. RF-Tines shares the public SDK contract; the handwritten DSP is independent.
