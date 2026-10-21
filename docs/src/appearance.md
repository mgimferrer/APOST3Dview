# Appearance

The **Appearance** window sets the look of the whole scene, shared by every open structure. It is what exported images and videos look like too.

<img class="window" src="images/appearance-window.png" alt="The Appearance window">

- **Look**: **Default**, or **Publication**, with the light coming from the viewer's direction and no shiny highlights, so every part of the molecule is lit the same way whatever its orientation.
- **Background:** the background color (exported images can also have a transparent background, see [Figures and videos](figures.md)).
- **Ambient occlusion**: soft contact shadows where atoms, bonds and surfaces meet, which give the picture depth. Exports compute it with many more samples than the live view.
- **Depth of field**: blurs what is far from the center of rotation, like a camera lens.
- **Advanced**: every fine-tuning slider. **Light** (its direction and the exposure), **Atoms and bonds** and **Surfaces** (ambient light, roughness, reflectance, light intensity; for surfaces also the rim), and, when they are on, **Ambient occlusion** and **Depth of field** (with a **Default** button each).

## Your own looks

Once you like a look, type a name and click **Save this look**. It appears as a button of its own next to **Default** and **Publication**, in every later session; its **×** deletes it. Each look is a small text file in your settings folder; **Open presets folder** shows it, to back looks up or give them to someone else:

| System | Folder |
|---|---|
| Windows | `%APPDATA%\APOST3Dview\data\presets` |
| macOS | `~/Library/Application Support/APOST3Dview/presets` |
| Linux | `~/.local/share/apost3dview/presets` |
