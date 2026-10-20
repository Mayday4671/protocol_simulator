@echo off
cd /d "%~dp0"
python -u musp_peer.py --log ..\artifacts\base-uav-trial\live-peer.log
pause
