@echo off
rem winarchy - shim para invocar la CLI desde cualquier shell.
rem Este archivo DEBE quedar con finales de linea CRLF (ver .gitattributes):
rem cmd.exe no parsea bloques if (...) else (...) con finales LF.
setlocal
set "WINARCHY_PS=powershell"
where pwsh >nul 2>nul && set "WINARCHY_PS=pwsh"
%WINARCHY_PS% -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0winarchy.ps1" %*
exit /b %ERRORLEVEL%
