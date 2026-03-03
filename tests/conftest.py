"""Test configuration for local source-tree imports."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

for rel in ("src", "extensions/whatsapp/src", "extensions/discord/src"):
    path = str(ROOT / rel)
    if path not in sys.path:
        sys.path.insert(0, path)
