@echo off
rem ===========================================================================
rem Module M5: browser-backend launcher (localhost only, 127.0.0.1:8765)
rem Docs: docs/interfaces/build-and-verify.md  |  docs/interfaces/http-api.md
rem Keep this file ASCII-only: Windows cmd reads .cmd in the OEM code page,
rem non-ASCII text here renders as mojibake and can break path matching.
rem ===========================================================================
setlocal
set "WORKBENCH_ROOT=%~dp0"
node "%WORKBENCH_ROOT%server\server.js"
pause
