@echo off
REM Launch the CDP-E2E dev stack detached from the caller's process tree
REM (cmd side): keeps the app window + cargo watchdog alive on its own.
start "md-dev" /min "D:\idea_work\my\MergeDrag\scripts\md-dev-e2e.bat"