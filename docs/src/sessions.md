# Sessions

A session (`.a3dview`) keeps everything you set up: the files open, the structure shown, hidden atoms and bonds, bonds created by hand and dashed bonds, measurements and where their labels are, the surfaces and how each is drawn, the APOST-3D values shown and their colors, the camera (and the view each structure was left with), the look of the scene, and the figure set up in **Export** (its panels, captions and layout). Opening it gives back the same picture.

- **Save** in the toolbar, then **Save session** (<kbd>Cmd</kbd>+<kbd>S</kbd>, or <kbd>Ctrl</kbd>+<kbd>S</kbd>), saves it; the first time it asks where. **Save session as...** saves it under another name. The window's title names the open session.
- Open a session like any other file. It replaces everything open.

A session points to your files rather than copying them, with paths relative to the session's folder, so keep it next to the files: moving or copying the whole folder is fine. It also records where each file was when it was saved, tried when the relative path doesn't lead to the file; before sharing a session with its files (with a colleague, or in a data deposit), you can delete the lines ending in `_absolute`, which name your own folders, and the session still opens. Orbitals and densities generated in the app are not stored but recalculated when the session opens, from the same files with the same settings.

A session also records the size and a checksum (SHA-256) of every file it uses. If a file is missing, or isn't the file the session was saved with (rerun, replaced or edited, even with the same atoms), APOST3Dview opens the rest and says so; when its atoms differ, it also says what it couldn't restore (hidden atoms, bonds, measurements). A session doesn't contain the files themselves: to reproduce a figure elsewhere, keep the session together with its files. A session saved by a newer version of APOST3Dview asks you to update.

By default, every exported image and video also gets its session (see [Figures and videos](figures.md)), so each figure keeps the setup that made it.

## Rendering a session from the command line

A session can be turned into its picture without opening a window:

```
apost3dview --render figure.a3dview figure.png
apost3dview --render figure.a3dview figure.png --size 2400x1500
```

A session whose figure is a panel figure (or a single image with a caption) gives that figure, at the size and resolution it records; otherwise the view is rendered at the size given (1920x1200 by default). The picture is the one the **Export** window saves, pixel for pixel, so this is a quick way to check that a session gives back its figure, or to remake figures from a script. The program is `apost3dview` on Linux, `APOST3Dview.exe` on Windows (which writes the picture but prints nothing), and on macOS it is inside the app: `/Applications/APOST3Dview.app/Contents/MacOS/APOST3Dview --render ...`.
