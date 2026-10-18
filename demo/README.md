# Demo project

A sun face that morphs into a boxy moon face, with 25 hand-placed dot pairs (eight around
each head, both eyes, three on the mouth, twelve pinned around the frame edge).

Both images are drawn by formulas in `crates/testkit/src/demo.rs`, so they are not
photographs and carry no licence. The project files are generated from the same code.

```
cargo run -p inbetween-cli -- render demo/demo.inbetween --out frames/
cargo run -p inbetween-cli -- frame demo/demo.inbetween --t 0.5 --out mid.png
cargo run -p inbetween-cli -- info demo/demo.inbetween
```

A test (`apps/inbetween-cli/tests/demo.rs`) fails if these files drift from the generator.
Regenerate them with:

```
cargo test -p inbetween-cli --test demo -- --ignored regenerate
```
