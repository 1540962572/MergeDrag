@echo off
REM Wrapper so `npm run tauri build` runs under the MSVC toolset environment
REM (git-bash strips vcvars quoting, and cargo is in the user dir not on PATH).
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul 2>&1
set "PATH=C:\Users\15409\.cargo\bin;%PATH%"
cd /d D:\idea_work\my\MergeDrag
npm run tauri build
exit /b %ERRORLEVEL%