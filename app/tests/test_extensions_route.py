"""Tests for extensions route — directory scan."""

from __future__ import annotations

from pathlib import Path
from unittest.mock import MagicMock

import pytest

from app.routes.extensions import _extensions_from_directory


def test_extensions_from_directory_returns_name_version(tmp_path: Path) -> None:
    ext_dir = tmp_path / "extensions" / "tts"
    ext_dir.mkdir(parents=True)
    (ext_dir / "pyproject.toml").write_text('''
[project]
name = "openagent-tts"
version = "1.2.3"

[project.entry-points."openagent.extensions"]
tts = "tts.plugin:TTSExtension"
''')

    result = _extensions_from_directory(tmp_path)
    assert len(result) == 1
    assert result[0]["name"] == "tts"
    assert result[0]["package"] == "openagent-tts"
    assert result[0]["version"] == "1.2.3"
    assert "TTSExtension" in result[0]["entry_point"]
