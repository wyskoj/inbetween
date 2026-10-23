<div align="center">

# Inbetween

**Image morphing and warping, in pure Rust.**

Place paired dots on two images. Inbetween renders every frame in between:<br>
shape deforms, colour dissolves.

[Features](#features) · [Quick start](#quick-start) · [Command line](#command-line) · [Docs](#docs)

![Rust](https://img.shields.io/badge/rust-1.85%2B-orange?logo=rust)
![License](https://img.shields.io/badge/license-Apache--2.0-blue)
![Status](https://img.shields.io/badge/status-pre--release-lightgrey)

</div>

<!-- TODO: add a screenshot or GIF of the app here, e.g. docs/img/hero.gif -->

## Features

- **Fast, native app.** Drag a dot and watch the morph update live.
- **Morph and warp.** Seven morph types, per-feature timing curves, hold frames, and multi-sequence projects that play back to back.
- **Effects and captions.** Cameras, lights, trails, masks, backgrounds and text.
- **Export anywhere.** PNG sequences, GIF, APNG and animated WebP out of the box, plus MP4, WebM and ProRes through ffmpeg.
- **Scriptable.** Every action is a command that the app, the `ib` CLI and an MCP server all share, so anything you can click you can automate.
- **Reliable.** The same project always renders the same frames, and bad files give clear errors instead of crashes.

## Quick start

You need [Rust](https://rustup.rs) 1.85 or newer. Inbetween isn't packaged yet, so build from source:

```sh
git clone https://github.com/wyskoj/inbetween
cd inbetween
cargo run --release -p inbetween -- demo/demo.inbetween
```

That opens the demo project, a sun face morphing into a moon face. Click an image to add a dot, then click the matching spot on the other image. Press play to see the result.

## Command line

The `ib` binary renders without a window, so it works in scripts and CI:

```sh
cargo build --release -p inbetween-cli

ib render demo/demo.inbetween --out frames/             # every frame as a PNG
ib frame  demo/demo.inbetween --t 0.5 --out mid.png     # one frame, halfway
ib info   demo/demo.inbetween                           # sequences, sizes, dot counts
ib commands                                             # everything you can run
```

For AI agents, `ib mcp` serves every command as an [MCP](https://modelcontextprotocol.io) tool:

```sh
claude mcp add inbetween -- ib mcp --project demo/demo.inbetween
```

## Docs

- [`docs/feature-spec.md`](docs/feature-spec.md): every feature and its command id
- [`docs/architecture.md`](docs/architecture.md): crate layout and how the pieces fit
- [`docs/decisions.md`](docs/decisions.md): design decisions and why
- [`docs/roadmap.md`](docs/roadmap.md): where the project is headed

## Contributing

Inbetween is built almost entirely with AI coding agents (Claude Code), guided by the rules in [`AGENTS.md`](AGENTS.md) and held to its test suite.

Run `cargo xtask ci` before sending a change. `AGENTS.md` has the ground rules for people and agents alike.

## License

Licensed under the [Apache License 2.0](LICENSE). Bundled assets are listed in [`ATTRIBUTION.md`](ATTRIBUTION.md).
