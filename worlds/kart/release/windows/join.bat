@echo off
rem SPDX-FileCopyrightText: The osage contributors
rem SPDX-License-Identifier: MIT OR Apache-2.0
rem Joins a race hosted on another computer of the same network (host.bat).
cd /d "%~dp0"
set /p HOST=Address of the computer that hosts (host.bat shows it, e.g. 192.168.1.20): 
kart-native.exe ws://%HOST%:7878 %*
