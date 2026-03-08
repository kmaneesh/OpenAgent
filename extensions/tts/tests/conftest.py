from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "src"

# Map the "tts" package directly to src/ so tests can import
# `from tts.plugin import ...` without a redundant src/tts/ subfolder.
import types
if "tts" not in sys.modules:
    _pkg = types.ModuleType("tts")
    _pkg.__path__ = [str(SRC)]
    _pkg.__package__ = "tts"
    _pkg.__file__ = str(SRC / "__init__.py")
    sys.modules["tts"] = _pkg
