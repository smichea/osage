@echo off
rem SPDX-FileCopyrightText: The osage contributors
rem SPDX-License-Identifier: MIT OR Apache-2.0
rem Races against the world's bots on this computer: starts the kart world,
rem then the game; closing the game stops the world. To join someone else's
rem world instead: kart-native.exe ws://THEIR-ADDRESS:7878
cd /d "%~dp0"
start "osage kart world" /min kart-server.exe
timeout /t 2 /nobreak >nul
kart-native.exe %*
taskkill /im kart-server.exe /f >nul 2>&1
