@echo off
rem SPDX-FileCopyrightText: The osage contributors
rem SPDX-License-Identifier: MIT OR Apache-2.0
rem Hosts a race for this computer and the others on the same network:
rem starts the kart world open to the network, shows this computer's address
rem for the others (they start join.bat, or open it in a browser on port
rem 8080 from a phone or a tablet), then the game. Closing the game
rem stops the world.
cd /d "%~dp0"
rem First the newest version, if there is one (kart-update.exe runs this file
rem again once updated; the whole block is read before it runs).
if not "%OSAGE_UPDATED%"=="1" if exist kart-update.exe (kart-update.exe --then "%~f0" %* & exit /b)
echo.
echo  The others start join.bat and type this computer's address:
for /f "tokens=2 delims=:" %%a in ('ipconfig ^| findstr /c:"IPv4"') do echo     %%a
echo.
echo  A phone or a tablet opens  http://THAT-ADDRESS:8080  in its browser,
echo  and anyone  http://THAT-ADDRESS:8080/leaderboard.html  for the records.
echo  If Windows asks, allow "kart-server" and "kart-web" on private networks.
echo.
rem The host's music, shared with the others, when there is a "music" folder.
set MUSIC=
if exist music\ (set MUSIC=--music music& echo  Sharing the music in the "music" folder on port 7880.& echo.)
start "osage kart world (shared)" /min kart-server.exe --listen 0.0.0.0:7878 --webtransport 0.0.0.0:7879 --records kart-records.txt --ghosts kart-ghosts %MUSIC%
start "osage kart web" /min kart-web.exe web 0.0.0.0:8080
timeout /t 3 /nobreak >nul
kart-native.exe %*
taskkill /im kart-server.exe /f >nul 2>&1
taskkill /im kart-web.exe /f >nul 2>&1
