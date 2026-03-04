"""Test configuration for local source-tree imports."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

for path in [
    str(ROOT),                               # openagent/ package at root
    str(ROOT / "app"),                       # app/ package
    str(ROOT / "extensions/whatsapp/src"),
    str(ROOT / "extensions/discord/src"),
]:
    if path not in sys.path:
        sys.path.insert(0, path)
