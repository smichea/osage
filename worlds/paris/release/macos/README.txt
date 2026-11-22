osage paris - macOS build
=========================

A walk on foot in a street of Paris, on osage, an open-source engine:
https://github.com/smichea/osage

A test world, not a game: one street (Amazon Lumberyard's Bistro) to walk
in. Your Mac draws the street's model for 13 metres round you; what lies
beyond is light baked beforehand by a path tracer, shown from where you
stand.

This build is for a Mac with Apple silicon (M1 or later).

To walk
-------
1. Download osage-paris-macos-arm64.zip and double-click it: you get a
   folder "osage-paris". Keep it where you like. The launcher runs in
   Terminal: if macOS asks whether Terminal may access the files in your
   Downloads (or Documents, or Desktop) folder, allow it.
2. Double-click walk.command. The first time, macOS stops it, because the
   programs are not signed by a registered Apple developer: click "Done",
   open System Settings > Privacy & Security, scroll down to the line
   saying "walk.command" was blocked, click "Open Anyway" and confirm.
   Once allowed, walk.command lifts macOS's quarantine mark from the
   other files of the folder, so that macOS does not stop each program in
   turn.
3. The first time, the folder downloads the street (its model, its
   textures and its baked light, about 1.3 GB). Then you stand in the
   street.
A Terminal window opens with the walk: it shows what the launcher does.
Leave it open while you walk; quitting the walk closes everything.

If macOS still refuses to open a program of the folder: open Terminal,
type  xattr -dr com.apple.quarantine  followed by a space, drag the
osage-paris folder onto the Terminal window, press Return, and
double-click walk.command again.

Controls
--------
  Arrows              walk             Shift (held)    run
  W A S D (Z Q S D)   look round
  Click               look with the mouse; Escape lets the mouse go

Graphics
--------
walk.command draws at tier T2. On a strong Mac, from Terminal in this
folder:  ./walk.command --tier T3  adds ambient occlusion, contact shadows and steadier edges.

Updates
-------
walk.command first looks for a newer version on GitHub and installs it
(checked against the published digest) before the walk starts. Offline,
the walk starts as it is. To turn this off, create an empty file named
no-update  in this folder.

Licences
--------
The program is free software, under MIT or Apache-2.0 (see LICENSE and
LICENSES/). Its source is the tag of this release on GitHub.
The street is "Amazon Lumberyard Bistro, Open Research Content Archive
(ORCA), 2017" (https://developer.nvidia.com/orca/amazon-lumberyard-bistro),
under CC BY 4.0 (https://creativecommons.org/licenses/by/4.0/), changed by
the osage contributors: exported for the client, and photographed into
the light the folder holds. What was made from it is under CC BY 4.0 too.
Authors and sources are in assets/manifest.toml.
