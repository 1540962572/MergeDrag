@echo off
REM Launch `tauri dev` under the MSVC toolset env with cargo on PATH.
REM (git-bash lacks cargo on PATH and its /usr/bin/link.exe would shadow MSVC's.)
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul
set "PATH=C:\Users\15409\.cargo\bin;%PATH%"
cd /d D:\idea_work\my\MergeDrag
npm run tauri dev
exit /b %ERRORLEVEL%