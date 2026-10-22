@echo off
cd /d "%~dp0"
python -u iot_walkthrough_peer.py --port 39162 --delay-ms 700
pause
