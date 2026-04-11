from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[1]
PYTHON_SOURCE = ROOT / "python"
SCRIPTS_SOURCE = ROOT / "scripts"

if str(PYTHON_SOURCE) not in sys.path:
    sys.path.insert(0, str(PYTHON_SOURCE))
if str(SCRIPTS_SOURCE) not in sys.path:
    sys.path.insert(0, str(SCRIPTS_SOURCE))
