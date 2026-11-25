"""Run on a fresh Linux CI runner with its native window system available."""
import os
import subprocess
import sys
import time
from pathlib import Path

binary = Path(sys.argv[1]).resolve()
data = Path(os.environ.get('XDG_DATA_HOME', Path.home() / '.local/share'))
marker = data / 'dev.mukade.desktop/window-theme'
start = time.time()
process = subprocess.Popen([binary], cwd=binary.parent)
try:
    code = process.wait(timeout=90)
    if code or not marker.exists() or marker.stat().st_mtime < int(start) or marker.read_text().strip() != 'dark':
        raise SystemExit(f'Native frontend did not complete its startup probe (exit {code})')
finally:
    if process.poll() is None:
        process.kill()
        process.wait()
