osage paris - Windows build
===========================

A walk on foot in a street of Paris, on osage, an open-source engine:
https://github.com/smichea/osage

A test world, not a game: one street (Amazon Lumberyard's Bistro) to walk
in. Your computer draws the street's model for 13 metres round you; what
lies beyond is light baked beforehand by a path tracer, shown from where
you stand.

To walk
-------
1. Unzip osage-paris-windows-x64.zip: you get one folder "osage-paris".
2. Double-click walk.bat. Windows may warn that the program is not signed:
   "More info", then "Run anyway".
3. The first time, the folder downloads the street (its model, its
   textures and its baked light, about 1.3 GB). Then you stand in the
   street.

Controls
--------
  Arrows              walk             Shift (held)    run
  W A S D (Z Q S D)   look round
  Click               look with the mouse; Escape lets the mouse go

Graphics
--------
walk.bat draws at tier T2, which every recent graphics card manages. On a
strong card:  walk.bat --tier T3  adds ambient occlusion, contact shadows and steadier edges.

Updates
-------
walk.bat first looks for a newer version on GitHub and installs it
(checked against the published digest) before the walk starts. Offline,
the walk starts as it is. To turn this off, create an empty file named
no-update  in this folder.

Licences
--------
The program is free software, under MIT or Apache-2.0 (see LICENSE and
LICENSES/). Its source is the tag of this release on GitHub.
dxcompiler.dll and dxil.dll are Microsoft's DirectX Shader Compiler
(https://github.com/microsoft/DirectXShaderCompiler), under its own
licences, in LICENSES/directx-shader-compiler/.
The street is "Amazon Lumberyard Bistro, Open Research Content Archive
(ORCA), 2017" (https://developer.nvidia.com/orca/amazon-lumberyard-bistro),
under CC BY 4.0 (https://creativecommons.org/licenses/by/4.0/), changed by
the osage contributors: exported for the client, and photographed into
the light the folder holds. What was made from it is under CC BY 4.0 too.
Authors and sources are in assets/manifest.toml.
