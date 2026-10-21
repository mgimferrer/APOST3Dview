# Sessions

A session (`.a3dview`) keeps everything you set up: the files open, the structure shown, hidden atoms and bonds, bonds created by hand and dashed bonds, measurements and where their labels are, the surfaces and how each is drawn, the APOST-3D values shown and their colors, the camera (and the view each structure was left with), the look of the scene, and the figure set up in **Export** (its panels, captions and layout). Opening it gives back the same picture.

- **Save** in the toolbar, then **Save session** (<kbd>Cmd</kbd>+<kbd>S</kbd>, or <kbd>Ctrl</kbd>+<kbd>S</kbd>), saves it; the first time it asks where. **Save session as...** saves it under another name. The window's title names the open session.
- Open a session like any other file. It replaces everything open.

A session points to your files rather than copying them, with paths relative to the session's folder, so keep it next to the files: moving or copying the whole folder is fine. Orbitals and densities generated in the app are not stored but recalculated when the session opens, from the same files with the same settings.

If a file is missing, or has changed since the session was saved (other atoms), APOST3Dview opens the rest and says what it couldn't restore. A session saved by a newer version of APOST3Dview asks you to update.

By default, every exported image and video also gets its session (see [Figures and videos](figures.md)), so each figure keeps the setup that made it.
