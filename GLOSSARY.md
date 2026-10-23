# Inbetween

The words Inbetween uses for the things a person works with when morphing one image into another.

## Language

**Sequence**:
One morph or warp in a project: a Start image, an End image and the dot set between them.

**Dot**:
A paired point that sits on the same feature in both images of a sequence; together the dots steer the shape change.
_Avoid_: point, landmark (for a placed dot), key point

**Face model**:
The project's list of face landmarks (eyes, brows, nose, mouth, jaw) and where each sits on a unit face box.

**Mark a face**:
Place the face model's dots on a face in both images of a sequence, replacing the sequence's dots.
_Avoid_: locate a face, detect a face (for placing dots)

**Detect faces**:
Find the faces in an image and report where they are, without changing any dots.
