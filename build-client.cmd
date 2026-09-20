@echo off
rem ===========================================================================
rem Module M5: portable Windows client build entry
rem Docs: docs/interfaces/build-and-verify.md  |  docs/interfaces/tauri-ipc.md
rem Steps: npm install -> npm run desktop:build -> copy exe to repo root.
rem Close the running exe first, or the final copy step fails.
rem Keep this file ASCII-only except the Chinese exe name below: Windows cmd
rem reads .cmd in the OEM code page, so extra non-ASCII text becomes mojibake.
rem ===========================================================================
setlocal
set "WORKBENCH_ROOT=%~dp0"
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
pushd "%WORKBENCH_ROOT%"
call npm install
call npm run desktop:build
if errorlevel 1 goto :end
copy /Y "%WORKBENCH_ROOT%desktop\src-tauri\target\release\personal-workbench.exe" "%WORKBENCH_ROOT%个人工作台.exe"
:end
popd
pause
