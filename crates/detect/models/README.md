# Bundled face models

Both files are Apache-2.0 (see `LICENSE-models.txt`) and are embedded by the default
`bundled-models` feature of `inbetween-detect`.

| File | What it is | Input | Outputs | SHA-256 |
|---|---|---|---|---|
| `blazeface.onnx` | Short-range face detector: 896 SSD anchors, a box and 6 keypoints per anchor | `1x128x128x3`, values -1 to 1 | `1x896x16` regressors, `1x896x1` score logits | `a48611f63726fcffb9e4e1491c020445a3bf2b83b062179735655a9d36aeef3e` |
| `face_landmarks.onnx` | 468-point face mesh on a 192 px crop | `1x192x192x3`, values 0 to 1 | `1x1x1x1404` (x, y, z per point, in crop pixels), `1x1x1x1` presence logit | `0064f74a7c76a43495d99570c794f8b54b261ef16b508c456dd6e53e998032f1` |

## Where they came from

Downloaded from the MediaPipe asset bucket on 2026-10-19:

| Source file | SHA-256 |
|---|---|
| `https://storage.googleapis.com/mediapipe-assets/face_detection_short_range.tflite` | `3bc182eb9f33925d9e58b5c8d59308a760f4adea8f282370e428c51212c26633` |
| `https://storage.googleapis.com/mediapipe-assets/face_landmark.tflite` | `1055cb9d4a9ca8b8c688902a3a5194311138ba256bcc94e336d8373a5f30c814` |

Converted once, offline (tf2onnx 1.17, TensorFlow 2.21, opset 13). No Python is needed to build
Inbetween:

```
python -m tf2onnx.convert --tflite face_detection_short_range.tflite --output blazeface.onnx --opset 13
python -m tf2onnx.convert --tflite face_landmark.tflite --output face_landmarks.onnx --opset 13
```

The attention/iris variant (`face_landmark_with_attention.tflite`) contains MediaPipe custom
operators that no converter handles, so it is not used. Pupils are placed at the mean of the four
eyelid points instead of the iris centre.

## Not used

dlib's 68-point model: it was trained on a data set whose licence forbids commercial use, so it
fails the permissive-licence rule (`docs/decisions.md`, D52).

## Checked against the originals

The unit test `reference::converted_models_match_the_tflite_reference` (in `src/detector.rs`)
runs both converted files in tract on two fixed inputs built from `tests/data/portrait_a.png`
and compares the outputs with `tests/data/reference/*.bin`, which the original TFLite models
produced from the same input tensors. Differences on 2026-10-20 (output values reach about 230):

| Output | Values | Max difference (tract vs TFLite) |
|---|---|---|
| detector regressors | 14336 | 0.0003 |
| detector score logits | 896 | 0.0007 |
| mesh points | 1404 | 0.0001 |
| mesh presence logit | 1 | 0.00002 |

onnxruntime agreed with TFLite to the same size, so the conversion is faithful. To regenerate the
references: run the test with `INBETWEEN_DUMP_REF=<dir>` (it writes the two input tensors), then
`python -I make_reference.py <folder with the .tflite and .onnx files> <dir> tests/data/reference`
(needs `tensorflow`, `onnxruntime`, `numpy`).

## Running on wasm32

`cargo xtask wasm` only compile-checks `wasm32-unknown-unknown`. To run the models for real, build
the `wasm_check` example for `wasm32-wasip1` and run it under Node's built-in WASI (Node 20 or
newer; `rustup target add wasm32-wasip1` once):

```
cargo build --release -p inbetween-detect --example wasm_check --target wasm32-wasip1
node crates/detect/examples/run_wasi.mjs target/wasm32-wasip1/release/examples/wasm_check.wasm \
    crates/detect/tests/data portrait_a.png
```

Checked on 2026-10-20: tract loads and optimises both models inside Node's WebAssembly engine,
finds the one face in `portrait_a.png` and prints the same score, box and landmark positions as
the native `cargo run --release -p inbetween-detect --example wasm_check -- <png>` (score 0.904,
box 274.0,136.5 to 356.8,230.4, nose tip 310.90,188.74). The whole run, load included, took about
200 ms. WASI has no threads, so this also exercises the inline fallback for the box detector's
extra windows.
