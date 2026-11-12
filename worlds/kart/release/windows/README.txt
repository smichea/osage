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

Racing friends on the same Wi-Fi
--------------------------------
1. One computer hosts: double-click host.bat. It shows this computer's
   address (like 192.168.1.20). If Windows' firewall asks, allow
   "kart-server" and "kart-web" on private networks.
2. Each of the others double-clicks join.bat and types that address.
   From a phone or a tablet (Safari on an iPad, Chrome on Android...):
   open  http://THAT-ADDRESS:8080  - the browser version of the game, with
   touch controls (left thumb steers, right thumb holds DRIFT or BRAKE).
   From a Linux or Mac computer with the source:
   cargo run --release -p osage-kart-native -- ws://THAT-ADDRESS:7878
3. Whoever arrives during the countdown takes a bot's place; whoever
   arrives during a race joins the next one.
If join.bat cannot connect: the Wi-Fi must be a "private" network in
Windows' settings on the host, and guest or public Wi-Fi often keeps
devices from seeing one another (use a phone's hotspot instead).

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
