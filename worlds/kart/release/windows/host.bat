@echo off
rem SPDX-FileCopyrightText: The osage contributors
rem SPDX-License-Identifier: MIT OR Apache-2.0
rem Hosts a race for this computer and the others on the same network:
rem starts the kart world open to the network, shows this computer's address
rem for the others (they start join.bat), then the game. Closing the game
rem stops the world.
cd /d "%~dp0"
echo.
echo  The others start join.bat and type this computer's address:
for /f "tokens=2 delims=:" %%a in ('ipconfig ^| findstr /c:"IPv4"') do echo     %%a
echo.
echo  If Windows asks, allow "kart-server" on private networks.
echo.
start "osage kart world (shared)" /min kart-server.exe --listen 0.0.0.0:7878 --webtransport 0.0.0.0:7879
timeout /t 3 /nobreak >nul
kart-native.exe %*
taskkill /im kart-server.exe /f >nul 2>&1
