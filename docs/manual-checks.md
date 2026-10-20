# Manual checks

Things the headless tests cannot show. Run them in the real app (`cargo run -p inbetween -- demo/demo.inbetween`), tick them off with the date and platform, and add a line when a milestone adds a tool that needs hands.

How to read the result: a check passes when the behaviour matches and nothing in the status bar turns red.

## M15: projects and files

- [ ] Drop two image files on the window: the New From Images form opens in drop order.
- [ ] Close the window with unsaved changes: the Save / Don't Save / Cancel prompt appears.
- [ ] File > Open Recent lists what you opened; a deleted file is greyed and clicking it removes it.
- [ ] File > Open Visually shows thumbnails for a folder of projects; double-click opens one.

## M16: source editing and captions

- [ ] Image > Crop: drag a corner past the picture edge, tilt with the round handle, press Enter. The picture is cropped and tilted, the dots still sit on their features, and Edit > Undo restores both.
- [ ] Image > Adjust: drag the Brightness slider continuously. The source panel updates live; OK keeps it as one undo step; Cancel puts everything back.
- [ ] Image > Adjust > Add filter > Gaussian Blur: set it, then Edit on its row, change the value, Run. Remove it with its button.
- [ ] Image > Caption on Movie: type text, drag it on the preview, change size, colours, border, shadow and alignment. Close; one Undo removes it all.
- [ ] Caption > Font > Browse: pick a TTF or OTF; the text redraws in it. Save, reopen: it still does. Choose a non-font file: a clear error, nothing changes.
- [ ] Image > Rotate 90 CW with the focus on each source panel (click in it first): only that image turns, its dots follow.
- [ ] F3 swaps the two images and their dots; F3 again restores.
- [ ] Image > Save To Library: the folder you used last time is pre-filled.
- [ ] Dots: press E, then drag a marquee on Image 1 (Shift adds, Alt subtracts); arrows move the selection 1 px, Ctrl+arrow 10 px; with nothing selected they step frames again.
- [ ] Dots: turn Auto partner off, click Image 1 then Image 2: the lone dot is magenta, hovering it makes a ring blink where its partner would go; Dots > Auto-Match fills it in.
- [ ] Dots: Alt+click puts the partner on the same spot; Dots > Auto Jump moves the pointer to the partner on the other image after a click.
- [ ] Lines: press L and click several points; Shift+click ends with a dot, right-click, Enter or Esc ends without one; hovering a line lights its whole group; dragging a line in the Edit tool moves the group.
- [ ] View: Ctrl+G, Ctrl+D, Ctrl+L, Ctrl+T, Ctrl+M toggle the grid, dots, lines, triangles and tracks; the Magnifier follows the pointer, N and M change its power; Dot Style and Dot Color change the dots.
- [ ] Dots: Ctrl+C, Ctrl+X, Ctrl+V move dots and their lines (pasted dots sit 10 px apart from the originals), Delete removes the selection, Ctrl+H, Ctrl+R and Ctrl+K open scale, rotate and skew.

## M18: movie and workspace

- [ ] Drag the two splitters (between the sources and the preview, and between Image 1 and Image 2); double-click a splitter does not need to work, but neither panel may vanish.
- [ ] F5 to F9 switch layouts; F9 opens the preview in a second window you can move to another monitor; closing that window returns to Full.
- [ ] F11 and Alt+Enter toggle fullscreen.
- [ ] Transport "Sequence" button switches to "Movie": play a two-sequence project across the join; the sequence list selection follows.
- [ ] Scrub with the % box ticked and with the wheel over the transport bar.
- [ ] Ctrl+=, Ctrl+-, Ctrl+0, Alt+Ctrl+0, Alt+Right and Alt+Down zoom the panel under the pointer; with the preview linked it follows.
- [ ] F8 frame strip on a long sequence: scrolling stays smooth and thumbnails fill in.
- [ ] Sequence > Movie Size: pick 1080p; the dots still sit on their features.

## M19: face detection

- [x] With a portrait pair in a project (public domain or your own, never committed), Face > Locate Face puts 38 dots on each image's own face; the dots sit on the eyes, brows, nose, mouth and jaw of both pictures (checked 2026-10-20 with the two NASA portraits in the real window).
- [x] Face > Locate All Faces on a group photo places one run of dots per face, left to right (checked 2026-10-20: the two portraits side by side give 76 dots, 38 on each person, in both panels).
- [x] A picture with no face leaves the dots alone and shows "no face found in image N" in the Locate Face form and in the status bar (checked 2026-10-20 in an offscreen render of the real shell, `INBETWEEN_UI_SHOTS=folder cargo test -p inbetween-ui-egui --test face_detect`; the demo project's sun and moon images have no face).
- [x] `inbetween out.inbetween --screenshot shot.png` shows the dot overlay on both panels (checked 2026-10-20 on the two NASA portraits).

## M14: MCP server

- [ ] `claude mcp add inbetween -- <path>/ib mcp --project demo/demo.inbetween`, then in Claude Code: the `inbetween` tools are listed, `frame_preview` shows a picture of the demo morph, `dots_add` then `frame_preview` shows the change, `edit_undo` takes it back, and `export_animation` writes a GIF.
- [ ] Closing the client ends the `ib mcp` process (no orphan left in the task list).
