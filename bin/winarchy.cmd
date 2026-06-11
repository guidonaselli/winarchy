@echo off
rem winarchy — shim para invocar la CLI desde cualquier shell
where pwsh >nul 2>nul
if %ERRORLEVEL% EQU 0 (
    pwsh -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0winarchy.ps1" %*
) else (
    powershell -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0winarchy.ps1" %*
)
