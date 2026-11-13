@echo off
setlocal
title Keycast Installer
cd /d "%~dp0"
echo ========================================================
echo   Keycast - Setup and Certificate Trust Helper
echo ========================================================
echo.
echo [1/3] Adding certificate to Current User Trusted Root store...
certutil.exe -user -addstore Root Keycast.cer >nul 2>&1
echo [2/3] Adding certificate to Current User Trusted Publisher store...
certutil.exe -user -addstore TrustedPublisher Keycast.cer >nul 2>&1
certutil.exe -user -addstore TrustedPeople Keycast.cer >nul 2>&1
echo [3/3] Removing Mark-of-the-Web (Zone.Identifier) if present...
powershell -NoProfile -ExecutionPolicy Bypass -Command "Get-ChildItem -LiteralPath '%~dp0' | Unblock-File" >nul 2>&1
echo.
echo Starting Keycast Setup...
start "" "%~dp0Keycast-Setup.exe"
