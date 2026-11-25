# Editing a molecule

The **Build** window changes a geometry: set a distance, an angle or a dihedral, turn a group about a bond, move atoms, delete atoms, change an element. Edits never touch a file or the structure opened from it. The first edit makes a copy, and every edit can be undone.

**Build** is in the toolbar once **Building and editing molecules** is ticked. APOST-3Dview asks at the first start, and **Features...** in the About window changes it later.

![Ethane turned to eclipsed in the Build window: the copy under the original, the blue frame and status bar of the Build mode](images/build-window.png)

## The Build mode

While the Build window is open, the view has a thin blue frame and the status bar is blue, starting with "Building:" and the name of the structure. Close the window (or click **Build** in the toolbar again) and the app is exactly as before.

Clicks select atoms as always, and the edits act on the selection, in the order the atoms were clicked. <kbd>Cmd</kbd>/<kbd>Ctrl</kbd>+<kbd>Z</kbd> undoes, <kbd>Cmd</kbd>/<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Z</kbd> redoes and <kbd>Delete</kbd> deletes the selected atoms. These keys work only while the Build window is open, and never while you type in a field.

## Edits go to a copy

Nothing happens when the window opens. At the first edit, APOST-3Dview makes a copy of the structure shown, named after it with "(edited)", places it right under the original in the Molecule window's list and shows it. The original stays as it was. Later edits go to the same copy while it is the structure shown.

The copy takes the geometry, the bonds (including those made with **Create bond**), how they are drawn, hidden atoms and measurements. It doesn't take the wavefunction, orbitals, surfaces or APOST-3D results: those belong to the calculation, and stay with the original.

The copy's name is in the **Name:** field at the top of the window. **Continue in a new copy** keeps the copy as it is now and goes on in a new copy of it, so you can keep several stages side by side.

The frames of a path can't be edited. Open each frame as a structure (Molecule window) to edit one geometry.

## Setting a distance, an angle or a dihedral

Select 2 atoms for a distance, 3 for an angle (the second is the vertex) or 4 for a dihedral. The value appears in a field. Type the new one and press <kbd>Enter</kbd> or **Set**. Distances are in the unit of Measure (Å or bohr), angles and dihedrals in degrees. Dihedrals have the IUPAC sign, as in Gaussian or ORCA: positive when, looking from the second atom to the third, the first bond turns clockwise to cover the last.

**Moves:** says what moves:

- **C2 and its side** (with C2 the last atom clicked, the default): the last atom and everything still connected to it once its bond is cut. For a dihedral, the bond cut is the middle one, so the whole group behind the third atom turns, every atom of it.
- **C2 alone**: the last atom only.
- **Both sides, half each**: each end with its side, by half the change.

The sides follow the bonds drawn. When the atoms are in one ring the sides can't move apart, and only the last atom moves (the window says so). To move a piece that the bonds join to the rest, hide the bond that joins it (Molecule window): it is cut there. To move two pieces together, join them with **Create bond**.

With 2 atoms selected, **Turn** turns the second atom's side about their bond by the angle typed, in degrees.

## Moving, deleting and changing atoms

- **Move**: moves the selected atoms by the x, y and z typed (empty means 0), along the molecule's own axes.
- **Delete selected atoms**: deletes them and their bonds. Measurements that used them go too.
- **Element:** turns the selected atoms into the element clicked, or the one typed in **Other element:** and **Change**. Only the element changes: the hydrogens stay as they were.

After an edit the bonds stay as they were, so a stretched bond stays drawn. **Bonds from distances** works them out again from the distances, as when a file is opened.

## Undo and saving

**Undo** and **Redo** go back and forth through the copy's edits, up to 200 of them, while the program runs.

**Save as .xyz...** writes the copy's coordinates, offering its name as the file name, so rename it first to get the file name you want. The comment line of the file says which file it was made from. After saving, the copy refers to that file, and its note in the Molecule window says "edited, saved".

A session keeps edited copies. A copy saved as a file is kept like any file. A copy not saved yet is written into the session itself, and such a session needs this version of APOST-3Dview or a later one (an older one says so instead of opening it). Closing an unsaved copy asks nothing, and undo can't bring it back, so save the ones you want to keep.
