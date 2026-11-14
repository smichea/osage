osage kart - macOS build
========================

An original kart race on osage, an open-source engine:
https://github.com/smichea/osage

This build is for a Mac with Apple silicon (M1 or later).

To play
-------
1. Download osage-kart-macos-arm64.zip and double-click it: you get a
   folder "osage-kart". Keep it where you like. The launchers run in
   Terminal: if macOS asks whether Terminal may access the files in your
   Downloads (or Documents, or Desktop) folder, allow it.
2. Double-click play.command. The first time, macOS stops it, because the
   programs are not signed by a registered Apple developer: click "Done",
   open System Settings > Privacy & Security, scroll down to the line
   saying "play.command" was blocked, click "Open Anyway" and confirm.
   Once allowed, play.command lifts macOS's quarantine mark from the
   other files of the folder, so that macOS does not stop each program in
   turn.
3. The first time, the folder downloads the content (photographed
   textures, models, sounds and music, a few hundred MB). Then the race
   starts with you and the bots; the next race follows. (Without the
   content - offline, say - the game still runs, in a plainer, procedural
   look and synthesised sound.)
A Terminal window opens with the game: it shows what the launcher does.
Leave it open while you play; quitting the game closes everything.

If macOS still refuses to open a program of the folder: open Terminal,
type  xattr -dr com.apple.quarantine  followed by a space, drag the
osage-kart folder onto the Terminal window, press Return, and
double-click play.command again.

Two players on one Mac
----------------------
Double-click play-two.command: the screen is split, player 1 above,
player 2 below.
  Player 1: W A S D to drive, Space or left Shift to drift, F or left
            Control for the item, and the first gamepad.
  Player 2: the arrows to drive, right Shift to drift, the / key for the
            item, and the second gamepad.
Between races player 1 picks a driver with A and D and a kart with W and S,
player 2 with the arrows (and a paint with the . key). R replays the whole race as television would.

Leaderboard
-----------
While a game runs, open  http://localhost:8080/leaderboard.html  in a
browser: every track's best laps, races and time trials, and the race
being run. When you host, the others open it at your address.

Updates
-------
play.command, play-two.command, host.command and join.command first look
for a newer version on GitHub and install it (checked against the
published digest) before the game starts. Offline, the game starts as it
is. To turn this off, create an empty file named  no-update  in this
folder.

Controls
--------
  Arrows or WASD      drive            Space / Shift   drift
  F or Control        use the item     C               change view
  L (the host, between races)  how hard the bots race: easy, normal, hard
  G (the host, between races)  the engine class: 50, 100, 150, 200 cc
  Q / E / X (held)    look left, right, back    fn+F12  screenshot
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
one track over and over, a time trial or a battle. The others' T is a
vote, which everyone sees.

Battle: in the Yard, a wide bowl of asphalt, every kart has three
lives. Each thing that hits you - a tyre, a cone, oil, a drone - or a
bumper takes one; after a hit you are spared for a moment. With no life
left you are out (your kart turns see-through). The last kart in wins, or
the one with most lives after two and a half minutes.

Mountains and canyons: Eagle Ridge climbs 40 m through switchbacks to a
tunnel under the summit, then comes down across a ravine; Red Gorge runs
between the walls of a canyon, over two bridges and through a tunnel.
Uphill your kart is slower, downhill faster. The High Roads cup races
both, with Pinewood and Gull Point.

Rain: some races are run in the rain (Gull Point most often). The road is
darker and shines, and it grips and brakes less: brake earlier, and a
drift slides further. Time trials are always dry.

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
drives. Your points and paints are kept in ~/.config/osage/kart-profile.txt.

The world keeps its records on each track: the ten best laps, races and time trials, one per driver,
shown between races (NEW! when you just set one). They are kept in
kart-records.txt in this folder, on the computer that hosts; updates leave
the file alone.

Your folder of settings
-----------------------
The game keeps your settings, profile, music and radio list in
~/.config/osage, a folder Finder hides: in Finder, Go > Go to Folder...
(Shift-Command-G), type  ~/.config/osage  and press Return.

Photo mode
----------
O freezes the picture and frees the camera: W, A, S, D move it, E and Q
up and down, the arrow keys turn it (hold Shift to go faster), R and F
zoom in and out, Z and X move the focus nearer and further, 1 and 2 open
and close the lens. The interface goes after a few seconds; fn+F12 saves
the picture in the game's folder. O again to go back. The race goes on
meanwhile, without you.

Sound and music
---------------
N switches the game's sounds on or off, M the music; B goes to the next
piece or radio station, Shift+B back. Escape opens the settings, where
"Music from" chooses where it comes from:
  - auto: the host's music when they share some, else the game's own;
  - the game's: five CC0 pieces that come with the content;
  - my folder: your own music files (ogg, mp3, flac, wav, m4a), put in
    ~/.config/osage/kart-music (the folder is made the first time);
  - the host's: the music the host shares (below);
  - radio: the stations you list in ~/.config/osage/kart-radios.txt, one
    a line:  station NAME | https://ADDRESS-OF-THE-STREAM
    and, if you turn it on in the settings ("Radio directory": a kind of
    music), stations of the radio-browser.info directory. It is off
    unless you choose a kind: asking the directory tells it what you
    search for, from your address.
What plays is shown at the bottom of the screen.

Sharing the host's music: put music files in a folder named  music  in
this folder on the host's computer before starting host.command. Everyone
hears the same piece at the same moment (if macOS asks whether kart-server
may accept incoming network connections, allow it; it shares on port
7880). Share only music you have the right to share with your friends.

Talking to one another
----------------------
Everyone in the race hears the others who talk. Your microphone stays
off until you choose, in the settings (Escape), "Voice": push to talk
(hold V while you speak) or an open microphone (it sends when you speak
louder than the room). With an open microphone, wear a headset, or the
others hear the game from your speakers too. "Mute" picks another player
(left, right) and Space mutes or unmutes them. Who is talking shows at
the bottom of the screen. The game runs from Terminal, so the first time
macOS asks whether Terminal may use the microphone: allow it (System
Settings > Privacy & Security > Microphone if you said no). The voices go
through the host's kart-server, on port 7881.

Racing friends on the same Wi-Fi
--------------------------------
1. One computer hosts: double-click host.command (host.bat on Windows).
   It shows this computer's address (like 192.168.1.20). If macOS asks
   whether kart-server and kart-web may accept incoming network
   connections, allow them.
2. Each of the others double-clicks join.command (join.bat on Windows)
   and types that address. If macOS asks whether Terminal may find
   devices on your local network, allow it.
   From a phone or a tablet (Safari on an iPad, Chrome on Android...):
   open  http://THAT-ADDRESS:8080  - the browser version of the game, with
   touch controls (left thumb steers, right thumb holds DRIFT or BRAKE).
   From a Linux computer with the source:
   cargo run --release -p osage-kart-native -- ws://THAT-ADDRESS:7878
3. Whoever arrives during the countdown takes a bot's place; whoever
   arrives during a race joins the next one.
If join.command cannot connect: on the host, macOS's firewall (System
Settings > Network > Firewall) must let kart-server in, and guest or
public Wi-Fi often keeps devices from seeing one another (use a phone's
hotspot instead).

Graphics
--------
The game chooses its tier for the Mac: T2 on Apple silicon. To force
one, start a launcher from Terminal with it: drag play.command onto a
Terminal window, type a space and  --tier T3  (for an M Pro, Max or
Ultra) or  --tier T1  (for less), and press Return. The traced light (T4)
is not in the macOS build.

When something goes wrong
-------------------------
The world's messages are in kart-server.log and kart-web.log in this
folder. If the game itself stops, it offers at the next start to send a
report (it asks first).

Licences
--------
The programs are free software: the game and the kart world under MIT or
Apache-2.0, the server under AGPL-3.0-or-later with exceptions (see
LICENSE, LICENSE-EXCEPTIONS.md and LICENSES/). Their source is the tag of
this release on GitHub. The content is CC0: authors and sources are in
assets/manifest.toml (the music: MintoDog, congusbongus, LushoGames and
Centurion_of_war, on OpenGameArt).
