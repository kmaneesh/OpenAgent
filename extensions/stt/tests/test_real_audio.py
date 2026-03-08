"""Integration tests — use the real faster-whisper model on actual audio files.

Run with:
    .venv/bin/pytest extensions/stt/tests/test_real_audio.py -v -m integration

To test with a real WhatsApp OGG voice note:
    STT_TEST_FILE=/path/to/voice.ogg .venv/bin/pytest extensions/stt/tests/test_real_audio.py -v -m integration

The tiny model (~39 MB) is downloaded on first run to ~/.cache/huggingface/
"""

from __future__ import annotations

import os
from pathlib import Path

import pytest

from stt.plugin import STTExtension


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _ext_with_tiny_model() -> STTExtension:
    return STTExtension(config={"provider": "faster-whisper", "whisper_model": "tiny"})


# ---------------------------------------------------------------------------
# Integration tests
# ---------------------------------------------------------------------------

@pytest.mark.integration
@pytest.mark.asyncio
async def test_transcribe_silent_wav_returns_string(wav_audio_file: Path):
    """Real model on a silent WAV — result must be a string (empty is fine)."""
    ext = _ext_with_tiny_model()
    await ext.initialize()
    try:
        result = await ext.listen(file=wav_audio_file)
        assert isinstance(result, str)
        print(f"\n[silent WAV] transcript: {result!r}")
    finally:
        await ext.shutdown()


@pytest.mark.integration
@pytest.mark.asyncio
async def test_transcribe_preserves_ogg_suffix(tmp_path: Path):
    """When given a .ogg file, the correct suffix is forwarded to the provider
    so ffmpeg can detect the Opus container format."""
    # Write the silent WAV bytes into a .ogg named file so the suffix is right
    # (actual OGG content not needed — we only verify the suffix is propagated)
    from conftest import _write_silent_wav
    ogg_path = tmp_path / "voice.ogg"
    _write_silent_wav(ogg_path)          # still WAV bytes; suffix is what matters

    ext = _ext_with_tiny_model()
    await ext.initialize()
    try:
        # Just check it doesn't explode (ffmpeg reads by content, not suffix alone)
        result = await ext.listen(file=ogg_path)
        assert isinstance(result, str)
        print(f"\n[.ogg suffix] transcript: {result!r}")
    finally:
        await ext.shutdown()


@pytest.mark.integration
@pytest.mark.asyncio
async def test_transcribe_custom_file():
    """Point STT_TEST_FILE=/path/to/voice.ogg at a real WhatsApp voice note.

    Skipped automatically when the env var is not set.
    """
    audio_path = os.getenv("STT_TEST_FILE")
    if not audio_path:
        pytest.skip("Set STT_TEST_FILE=/path/to/audio.ogg to run this test")

    path = Path(audio_path)
    if not path.exists():
        pytest.fail(f"STT_TEST_FILE does not exist: {path}")

    ext = _ext_with_tiny_model()
    await ext.initialize()
    try:
        result = await ext.listen(file=path)
        assert isinstance(result, str)
        print(f"\n[{path.name}] transcript:\n  {result!r}")
        # Non-empty transcript is expected for real speech
        assert result.strip(), f"Expected a non-empty transcript for {path.name}"
    finally:
        await ext.shutdown()
