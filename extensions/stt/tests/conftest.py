from __future__ import annotations

import struct
import sys
import wave
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "src"
TTS_SRC = ROOT.parent / "tts" / "src"

# Map "stt" and "tts" packages directly to their src/ dirs so tests can import
# `from stt.plugin import ...` / `from tts.plugin import ...` without redundant subfolders.
import types

def _register_flat_pkg(name: str, src: Path) -> None:
    if name not in sys.modules:
        pkg = types.ModuleType(name)
        pkg.__path__ = [str(src)]
        pkg.__package__ = name
        pkg.__file__ = str(src / "__init__.py")
        sys.modules[name] = pkg

_register_flat_pkg("stt", SRC)
_register_flat_pkg("tts", TTS_SRC)


def _write_silent_wav(path: Path, duration_s: float = 1.0, sample_rate: int = 16000) -> None:
    """Write a minimal valid mono 16-bit PCM WAV file containing silence."""
    n_frames = int(sample_rate * duration_s)
    with wave.open(str(path), "w") as w:
        w.setnchannels(1)
        w.setsampwidth(2)          # 16-bit
        w.setframerate(sample_rate)
        w.writeframes(struct.pack(f"<{n_frames}h", *([0] * n_frames)))


@pytest.fixture()
def wav_audio_file(tmp_path: Path) -> Path:
    """A temporary silent WAV file usable as a real audio input for STT tests."""
    path = tmp_path / "test_audio.wav"
    _write_silent_wav(path)
    return path
