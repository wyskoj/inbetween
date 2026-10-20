"""Run the ORIGINAL TFLite models on the inputs the Rust test dumped and save their outputs.

usage: python -I make_reference.py MODELS_DIR DUMP_DIR OUT_DIR
Also runs the converted ONNX files in onnxruntime as a second opinion and prints the diffs.
"""
import os
import sys

os.environ["TF_CPP_MIN_LOG_LEVEL"] = "3"
import numpy as np
import onnxruntime as ort
import tensorflow as tf

models, dump, out = sys.argv[1:4]
os.makedirs(out, exist_ok=True)


def run_tflite(path, x):
    it = tf.lite.Interpreter(model_path=path)
    it.allocate_tensors()
    it.set_tensor(it.get_input_details()[0]["index"], x)
    it.invoke()
    return [it.get_tensor(d["index"]) for d in it.get_output_details()]


def run_onnx(path, x):
    s = ort.InferenceSession(path)
    return s.run(None, {s.get_inputs()[0].name: x})


def save(name, arr):
    np.asarray(arr, dtype="<f4").tofile(os.path.join(out, name + ".bin"))


def by_size(outs, size):
    return [o for o in outs if o.size == size][0]


blaze_in = np.fromfile(os.path.join(dump, "blaze_input.bin"), dtype="<f4").reshape(1, 128, 128, 3)
mesh_in = np.fromfile(os.path.join(dump, "mesh_input.bin"), dtype="<f4").reshape(1, 192, 192, 3)

tb = run_tflite(os.path.join(models, "face_detection_short_range.tflite"), blaze_in)
ob = run_onnx(os.path.join(models, "blazeface.onnx"), blaze_in)
tm = run_tflite(os.path.join(models, "face_landmark.tflite"), mesh_in)
om = run_onnx(os.path.join(models, "face_landmarks_468.onnx"), mesh_in)

pairs = [
    ("blaze_regressors", by_size(tb, 896 * 16), by_size(ob, 896 * 16)),
    ("blaze_scores", by_size(tb, 896), by_size(ob, 896)),
    ("mesh_points", by_size(tm, 1404), by_size(om, 1404)),
    ("mesh_presence", by_size(tm, 1), by_size(om, 1)),
]
for name, ref, onnx in pairs:
    save(name, ref)
    print(f"{name}: {ref.size} values, tflite vs onnxruntime max diff {np.abs(ref.ravel() - onnx.ravel()).max():.6f}, "
          f"range {ref.min():.3f}..{ref.max():.3f}")
