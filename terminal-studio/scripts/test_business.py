"""Run Rust application tests with the same Common Controls v6 dependency as the app.

Tauri attaches this manifest to application binaries, not Rust lib-test executables.
Only the generated test executable's manifest resource is updated.
"""
import ctypes
import json
import subprocess
import sys
import time
from pathlib import Path

project = Path(__file__).resolve().parents[1]
build = subprocess.run(
    ["cargo", "test", "--lib", "--features", "e2e", "--no-run", "--message-format=json"],
    cwd=project / "src-tauri", stdout=subprocess.PIPE, text=True,
)
if build.returncode:
    print(build.stdout)
    sys.exit(build.returncode)
artifacts = [json.loads(line) for line in build.stdout.splitlines() if line.startswith("{")]
executable = next(Path(a["executable"]) for a in artifacts
                  if a.get("reason") == "compiler-artifact" and a.get("executable")
                  and a["target"]["name"] == "simulated_terminal_lib")
manifest = b'''<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
<assemblyIdentity version="1.0.0.0" processorArchitecture="amd64" name="Simterm.Tests" type="win32"/>
<dependency><dependentAssembly><assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/></dependentAssembly></dependency>
</assembly>'''
kernel = ctypes.WinDLL("kernel32", use_last_error=True)
kernel.BeginUpdateResourceW.argtypes = [ctypes.c_wchar_p, ctypes.c_int]
kernel.BeginUpdateResourceW.restype = ctypes.c_void_p
kernel.UpdateResourceW.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_ushort, ctypes.c_void_p, ctypes.c_uint]
kernel.EndUpdateResourceW.argtypes = [ctypes.c_void_p, ctypes.c_int]
buffer = ctypes.create_string_buffer(manifest)
for attempt in range(6):
    handle = kernel.BeginUpdateResourceW(str(executable), False)
    if handle:
        if not kernel.UpdateResourceW(handle, 24, 1, 1033, buffer, len(manifest)):
            error = ctypes.get_last_error()
            kernel.EndUpdateResourceW(handle, True)
        elif kernel.EndUpdateResourceW(handle, False):
            break
        else:
            error = ctypes.get_last_error()
    else:
        error = ctypes.get_last_error()
    if attempt == 5:
        raise ctypes.WinError(error)
    # The Windows scanner can briefly hold a newly linked executable.
    time.sleep(1)
sys.exit(subprocess.call([str(executable), *(sys.argv[1:] or ["business_tests"]), "--nocapture"], cwd=project / "src-tauri"))
