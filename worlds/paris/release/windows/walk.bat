@echo off
rem SPDX-FileCopyrightText: The osage contributors
rem SPDX-License-Identifier: MIT OR Apache-2.0
rem A walk in the street, on this computer alone: the street's model round
rem the walker, lit by the light baked into the street's folder.
cd /d "%~dp0"
rem First the newest version and the street, if they are not here yet
rem (paris-update.exe runs this file again once updated; the whole block is
rem read before it runs).
if not "%OSAGE_UPDATED%"=="1" if exist paris-update.exe (paris-update.exe --then "%~f0" %* & exit /b)
paris.exe --offline --ring --block-probes %*
rem Something went wrong: the window stays, for the message to be read.
if errorlevel 1 pause
