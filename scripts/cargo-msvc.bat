@echo off
REM Wrapper so cargo runs under the MSVC toolset environment (git-bash strips vcvars quoting).
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul 2>&1
set "PATH=C:\Users\15409\.cargo\bin;%PATH%"
cd /d D:\idea_work\my\MergeDrag
cargo %*
exit /b %ERRORLEVEL%