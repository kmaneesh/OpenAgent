from __future__ import annotations

from pathlib import Path
from unittest.mock import AsyncMock, MagicMock

import pytest

from stt.providers.whisper import FasterWhisperProvider


@pytest.mark.asyncio
async def test_faster_whisper_transcribe_uses_vad_filter_and_returns_text():
    provider = FasterWhisperProvider(model_size="small")
    fake_segment_1 = MagicMock()
    fake_segment_1.text = " hello "
    fake_segment_2 = MagicMock()
    fake_segment_2.text = "world "
    fake_model = MagicMock()
    fake_model.transcribe.return_value = ([fake_segment_1, fake_segment_2], {})

    async def fake_get_model():
        return fake_model

    provider._get_model = fake_get_model  # type: ignore[method-assign]
    tmp_path = Path("/tmp/openagent-stt-test.wav")
    provider._write_temp_audio = AsyncMock(return_value=tmp_path)  # type: ignore[method-assign]
    provider._safe_remove = lambda _p: None  # type: ignore[method-assign]

    text = await provider.transcribe(b"audio-bytes")
    assert text == "hello world"
    fake_model.transcribe.assert_called_once()
    assert fake_model.transcribe.call_args.kwargs["vad_filter"] is True
