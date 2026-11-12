@echo off
rem SPDX-FileCopyrightText: The osage contributors
rem SPDX-License-Identifier: MIT OR Apache-2.0
rem Joins a race hosted on another computer of the same network (host.bat).
cd /d "%~dp0"
rem First the newest version, if there is one (kart-update.exe runs this file
rem again once updated; the whole block is read before it runs).
if not "%OSAGE_UPDATED%"=="1" if exist kart-update.exe (kart-update.exe --then "%~f0" %* & exit /b)
set /p HOST=Address of the computer that hosts (host.bat shows it, e.g. 192.168.1.20): 
kart-native.exe ws://%HOST%:7878 %*
