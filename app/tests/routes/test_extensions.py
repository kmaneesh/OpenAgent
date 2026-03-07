"""Tests for app.routes.extensions — directory scan."""

from __future__ import annotations

from pathlib import Path

import pytest

from app.routes.extensions import _extensions_from_directory


def test_extensions_from_directory_empty(tmp_path: Path) -> None:
    result = _extensions_from_directory(tmp_path)
    assert result == []


def test_extensions_from_directory_missing(tmp_path: Path) -> None:
    # No extensions dir
    result = _extensions_from_directory(tmp_path)
    assert result == []


def test_extensions_from_directory_finds_tts_stt(tmp_path: Path) -> None:
    # Create extensions/tts and extensions/stt with pyproject.toml
    for name, pkg in [("tts", "openagent-tts"), ("stt", "openagent-stt")]:
        ext_dir = tmp_path / "extensions" / name
        ext_dir.mkdir(parents=True)
        (ext_dir / "pyproject.toml").write_text(f'''
[project]
name = "{pkg}"
version = "0.1.0"

[project.entry-points."openagent.extensions"]
{name} = "{name}.plugin:Extension"
''')

    result = _extensions_from_directory(tmp_path)
    assert len(result) == 2
    names = [r["name"] for r in result]
    assert "stt" in names
    assert "tts" in names
    assert all(r["status"] == "registered" for r in result)
