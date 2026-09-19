@echo off
setlocal
set "WORKBENCH_ROOT=%~dp0"
node "%WORKBENCH_ROOT%server\server.js"
pause
