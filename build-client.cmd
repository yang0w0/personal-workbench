@echo off
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
