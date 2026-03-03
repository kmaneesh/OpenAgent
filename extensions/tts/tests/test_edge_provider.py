from __future__ import annotations

from unittest.mock import patch

import pytest

from tts.providers.edge import EdgeProvider


class _FakeCommunicate:
    def __init__(self, *args, **kwargs):
        self.args = args
        self.kwargs = kwargs

    async def stream(self):
        yield {"type": "audio", "data": b"abc"}
        yield {"type": "word_boundary", "offset": 1}
        yield {"type": "audio", "data": b"def"}


@pytest.mark.asyncio
async def test_edge_provider_generate_aggregates_audio_chunks():
    provider = EdgeProvider()
    with patch("tts.providers.edge.edge_tts.Communicate", _FakeCommunicate):
        audio = await provider.generate("hello", voice_id="en-US-AriaNeural")
    assert audio == b"abcdef"
