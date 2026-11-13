@echo off
rem SPDX-FileCopyrightText: The osage contributors
rem SPDX-License-Identifier: MIT OR Apache-2.0
rem Two players on this computer, the screen split (D-086), against the
rem world's bots: player 1 W A S D, Space to drift, F for the item, the first
rem gamepad; player 2 the arrows, right Shift, right Ctrl, the second gamepad.
cd /d "%~dp0"
rem First the newest version, if there is one (kart-update.exe runs this file
rem again once updated; the whole block is read before it runs).
if not "%OSAGE_UPDATED%"=="1" if exist kart-update.exe (kart-update.exe --then "%~f0" %* & exit /b)
start "osage kart world" /min kart-server.exe --records kart-records.txt --ghosts kart-ghosts
rem The world's leaderboard in a browser: http://localhost:8080/leaderboard.html
start "osage kart web" /min kart-web.exe web 127.0.0.1:8080
timeout /t 2 /nobreak >nul
kart-native.exe --players 2 %*
taskkill /im kart-server.exe /f >nul 2>&1
taskkill /im kart-web.exe /f >nul 2>&1
