osage kart - Windows build
==========================

An original kart race on osage, an open-source engine:
https://github.com/smichea/osage

To play
-------
1. Unzip osage-kart-windows-x64.zip and osage-kart-content.zip into the
   same folder: you get one folder "osage-kart" with the two programs and
   an "assets" folder in it. (Without the content the game still runs, in
   a plainer, procedural look and synthesised sound.)
2. Double-click play.bat. Windows may warn that the program is not signed:
   "More info", then "Run anyway".
   The race starts with you and the bots; the next race follows.

Controls
--------
  Arrows or WASD      drive            Space / Shift   drift
  F or Ctrl           use the item     C               change view
  Q / E / X (held)    look left, right, back           F12  screenshot
  A gamepad works too: triggers or South/East to drive, West to drift,
  North to use the item, the right stick to look round.

Racing a friend
---------------
On one computer, start kart-server.exe with
    kart-server.exe --listen 0.0.0.0:7878 --webtransport 0.0.0.0:7879
and let Windows' firewall accept it on the private network. On the other:
    kart-native.exe ws://ADDRESS-OF-THE-FIRST:7878

Graphics
--------
kart-native.exe --tier T1 for a modest GPU, T2 by default, T3 or T4 for a
strong one.

Licences
--------
The programs are free software: the game and the kart world under MIT or
Apache-2.0, the server under AGPL-3.0-or-later with exceptions (see
LICENSE, LICENSE-EXCEPTIONS.md and LICENSES/). Their source is the tag of
this release on GitHub. The content is CC0: authors and sources are in
assets/manifest.toml.
