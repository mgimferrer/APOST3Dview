# Figures and videos

The **Export** window saves the current view, exactly as it is on screen (labels, values, legend, ambient occlusion and depth of field included), as a PNG image or as a looping animated GIF.

<img class="window" src="images/export-window.png" alt="The Export window">

## Images

- **DPI** (the default): give the resolution and the **Figure width:** in inches, or click **Single column (3.25 in)** or **Double column (6.75 in)**; the height follows the shape of the view. The physical size is stored in the PNG, so the figure comes in at the right size in a document. 300 DPI is the usual journal minimum.
- **Custom (pixels)**: give the **Width:** and **Height:** in pixels, and a **Supersample:** factor (rendering at 2 or 4 times the size and scaling down smooths every edge).
- **Transparent background**: no background, for putting the molecule on a slide or over another picture.

The window shows the size of the image that will be made. Click **Render and save PNG...** and choose where to save it.

## Turning videos (GIF)

Under **Animation (GIF)**, give the **Width:** in pixels, the **Duration:** of one full turn, and the **Frame rate:**, then click **Render and save GIF...**. The molecule makes one full turn from the current view and the video loops seamlessly. It turns in the direction of the last arrow key pressed (horizontally if none was), about the center of the view, so leave some margin around the molecule. GIF files play directly in PowerPoint, Keynote and Google Slides. They use the background color of the **Appearance** window.

## The session beside each figure

With **Also save the session** ticked (the default), every export also writes a session next to the image: `figure.a3dview` next to `figure.png`. Opening it later gives back exactly the same picture, to change it or make it again at another size. See [Sessions](sessions.md).
