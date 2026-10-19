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
