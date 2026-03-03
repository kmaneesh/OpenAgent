from __future__ import annotations

from pathlib import Path
from unittest.mock import AsyncMock

import pytest

from stt.plugin import STTExtension


@pytest.mark.asyncio
async def test_stt_extension_defaults_to_faster_whisper():
    ext = STTExtension(config={})
    await ext.initialize()
    assert ext.get_status()["provider"] == "faster-whisper"
    await ext.shutdown()


@pytest.mark.asyncio
async def test_listen_with_audio_data_delegates_to_provider():
    ext = STTExtension(config={})
    await ext.initialize()
    provider = ext._provider
    assert provider is not None
    provider.transcribe = AsyncMock(return_value="hello")  # type: ignore[method-assign]
    text = await ext.listen(audio_data=b"audio")
    assert text == "hello"
    await ext.shutdown()


@pytest.mark.asyncio
async def test_listen_with_file_reads_and_transcribes(tmp_path: Path):
    file_path = tmp_path / "sample.wav"
    file_path.write_bytes(b"audio")
    ext = STTExtension(config={})
    await ext.initialize()
    provider = ext._provider
    assert provider is not None
    provider.transcribe = AsyncMock(return_value="from-file")  # type: ignore[method-assign]
    text = await ext.listen(file=file_path)
    assert text == "from-file"
    await ext.shutdown()


async def _fake_stream():
    yield b"a"
    yield b"b"
    yield b""
    yield b"c"
    yield b"d"


@pytest.mark.asyncio
async def test_listen_with_stream_chunks_audio():
    ext = STTExtension(config={})
    await ext.initialize()
    provider = ext._provider
    assert provider is not None
    provider.transcribe = AsyncMock(side_effect=["first", "second"])  # type: ignore[method-assign]
    text = await ext.listen(stream=_fake_stream(), chunk_bytes=2)
    assert text == "first second"
    await ext.shutdown()
