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

Updates
-------
play.bat, host.bat and join.bat first look for a newer version on GitHub
and install it (checked against the published digest) before the game
starts. Offline, the game starts as it is. To turn this off, create an
empty file named  no-update  in this folder.

Controls
--------
  Arrows or WASD      drive            Space / Shift   drift
  F or Ctrl           use the item     C               change view
  Q / E / X (held)    look left, right, back           F12  screenshot
  A gamepad works too: triggers or South/East to drive, West to drift,
  North to use the item, the right stick to look round.

Driving well pays:
  - hold a drift through a bend: the sparks turn blue, orange, then
    violet; release for a mini-turbo, longer at each colour;
  - over the crest on the start straight, flat out, the kart takes off:
    press drift in the air for a trick and a boost on landing;
  - press the throttle as the lights go out for a boost; press it too
    early and the engine stalls;
  - stay right behind another kart for a moment: its wake slings you past.

Items come from the glass crates across the road. The further back you
are, the stronger they get: a drone that flies to the leader, a push from
the pit crew (a boost nothing can touch), rockets. Tyres, a stack of three
tyres, cones and oil can be held behind your kart: keep F down and they
shield you from a tyre thrown from behind; let go of F to throw or drop
them. A bumper knocks aside the karts right beside you.

Four tracks, one cup: the world races the Grand Tour - the Paddock, Gull
Point along the cliffs, Pinewood through the forest at dusk and Harbour
Town round the houses - and gives points for each place (10, 8, 6, 5, 4,
3, 2, 1). The standings show between races, the podium after the last.
Between races the host presses T to choose what to race next: the cup,
one track over and over, or a time trial on a track. The others' T is a
vote, which everyone sees.

Time trials: everyone alone against the clock - no bots, no items, and
the karts pass through one another. Your best run comes back as a ghost
(a pale kart you can see through) in the next trials, with the fastest
runs of the others: beat it and it is replaced. Ghosts are kept in the
kart-ghosts folder, on the computer that hosts; updates leave it alone.

Who drives, and in what: between races (on the grid, or on the results),
left and right choose the driver, up and down the kart. Light drivers
are quick off the line but pushed around; heavy ones are slow to get
going and hard to shift. The Sprint leaps out of bends, the Cruiser holds
its speed, the Drifter slides. No pair is faster than the others: pick
the one you like to drive. On the grid your kart changes at once; after
a race, from the next one. The game remembers your driver and kart.

P paints your kart. The eight team colours are yours from the start;
points won in cups unlock silver (20 points), pearl (50), midnight (90)
and gold (140). Paints only change how a kart looks, never how it
drives. Your points and paints are kept in %APPDATA%\osage\kart-profile.txt.

The world keeps its records on each track: the ten best laps, races and time trials, one per driver,
shown between races (NEW! when you just set one). They are kept in
kart-records.txt in this folder, on the computer that hosts; updates leave
the file alone.

Sound and music
---------------
N switches the game's sounds on or off, M the music; B goes to the next
piece or radio station, Shift+B back. Escape opens the settings, where
"Music from" chooses where it comes from:
  - auto: the host's music when they share some, else the game's own;
  - the game's: five CC0 pieces that come with the content;
  - my folder: your own music files (ogg, mp3, flac, wav, m4a), put in
    %APPDATA%\osage\kart-music (the folder is made the first time);
  - the host's: the music the host shares (below);
  - radio: the stations you list in %APPDATA%\osage\kart-radios.txt, one
    a line:  station NAME | https://ADDRESS-OF-THE-STREAM
    and, if you turn it on in the settings ("Radio directory": a kind of
    music), stations of the radio-browser.info directory. It is off
    unless you choose a kind: asking the directory tells it what you
    search for, from your address.
What plays is shown at the bottom of the screen.

Sharing the host's music: put music files in a folder named  music  in
this folder on the host's computer before starting host.bat. Everyone
hears the same piece at the same moment (if Windows' firewall asks, allow
"kart-server" on private networks; it shares on port 7880). Share only
music you have the right to share with your friends.

Talking to one another
----------------------
Everyone in the race hears the others who talk. Your microphone stays
off until you choose, in the settings (Escape), "Voice": push to talk
(hold V while you speak) or an open microphone (it sends when you speak
louder than the room). With an open microphone, wear a headset, or the
others hear the game from your speakers too. "Mute" picks another player
(left, right) and Space mutes or unmutes them. Who is talking shows at
the bottom of the screen. If Windows' firewall asks, allow "kart-server"
on private networks (the voices go through it, on port 7881).

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
this release on GitHub. dxcompiler.dll and dxil.dll are Microsoft's
DirectX Shader Compiler (https://github.com/microsoft/DirectXShaderCompiler),
under its own licences, in LICENSES/directx-shader-compiler/. The content is CC0: authors and sources are in
assets/manifest.toml (the music: MintoDog, congusbongus, LushoGames and
Centurion_of_war, on OpenGameArt).
