@echo off
REM CDP-E2E dev stack: like tauri-dev-msvc.bat but with WebView2 remote
REM debugging on port 9336 and stdout logged to md-dev.log at repo root.
set "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9336"
call "D:\idea_work\my\MergeDrag\scripts\tauri-dev-msvc.bat" > "D:\idea_work\my\MergeDrag\md-dev.log" 2>&1