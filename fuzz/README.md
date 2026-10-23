# Fuzzing the file parsers

Targets: `project` (`.inbetween`) and `dots` (`.inbetweendots`). Property: reading never
panics, and anything that reads writes and reads back equal.

```
cargo install cargo-fuzz
cargo +nightly fuzz run project fuzz/seeds/project -- -max_total_time=120
cargo +nightly fuzz run dots fuzz/seeds/dots -- -max_total_time=120
```

`fuzz/seeds/` holds small valid files to start from. CI does not run these targets; run
them by hand after changing the parsers or the model they read. A crash prints a reproducer under
`fuzz/artifacts/`; turn it into a regression test in `crates/format` before fixing.

`libfuzzer-sys` is licensed `(MIT OR Apache-2.0) AND NCSA`. NCSA is permissive, but it is
not in `deny.toml`'s allow-list, so this crate stays outside the main workspace and is
never linked into Inbetween.
