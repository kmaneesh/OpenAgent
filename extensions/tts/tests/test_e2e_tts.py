"""End-to-end TTS test.

Converts a text string to audio using the real Edge TTS provider and writes
the result to disk so you can listen to it.

Usage
-----
# With default text:
    .venv/bin/pytest extensions/tts/tests/test_e2e_tts.py -v -s -m integration

# With custom text:
    TTS_TEST_TEXT="Hello, this is a test." \\
        .venv/bin/pytest extensions/tts/tests/test_e2e_tts.py -v -s -m integration

# Custom output path (default: data/artifacts/tts_e2e.mp3):
    TTS_TEST_TEXT="Hi there" TTS_OUT_FILE=data/artifacts/my_voice.mp3 \\
        .venv/bin/pytest extensions/tts/tests/test_e2e_tts.py -v -s -m integration

The voice, speed, and volume are read from config/openagent.yaml (tts section).
"""

from __future__ import annotations

import os
from pathlib import Path

import pytest

from tts.plugin import TTSExtension


ROOT = Path(__file__).resolve().parents[3]  # repo root
DEFAULT_TEXT = "Hello! This is a text-to-speech end-to-end test."


def _test_text() -> str:
    return os.getenv("TTS_TEST_TEXT", DEFAULT_TEXT).strip()


def _out_path() -> Path:
    raw = os.getenv("TTS_OUT_FILE", "")
    return Path(raw) if raw else ROOT / "data" / "artifacts" / "tts_e2e.mp3"


@pytest.mark.integration
@pytest.mark.asyncio
async def test_text_to_audio():
    """
    Synthesises a text string with the configured TTS provider and writes
    the resulting audio to disk.

    Set TTS_TEST_TEXT="..." to customise the input.
    Set TTS_OUT_FILE=path/to/output.mp3 to change where audio is saved.
    Voice/speed/volume come from config/openagent.yaml tts section.
    """
    from openagent.config import load_config

    cfg = load_config(ROOT / "config" / "openagent.yaml")

    ext = TTSExtension(config={
        "provider": cfg.tts.provider,
        "voice":    cfg.tts.voice,
        "speed":    cfg.tts.speed,
        "volume":   cfg.tts.volume,
        "api_key":  cfg.tts.api_key,
        "group_id": cfg.tts.group_id,
    })
    await ext.initialize()

    text = _test_text()
    out = _out_path()
    out.parent.mkdir(parents=True, exist_ok=True)

    print(f"\n[e2e] Provider : {cfg.tts.provider}")
    print(f"[e2e] Voice    : {cfg.tts.voice}")
    print(f"[e2e] Text     : {text!r}")

    try:
        audio = await ext.speak(text)
    finally:
        await ext.shutdown()

    assert audio, "Expected non-empty audio bytes"
    out.write_bytes(audio)
    print(f"[e2e] Audio written → {out}  ({len(audio):,} bytes)")
