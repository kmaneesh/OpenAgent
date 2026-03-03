from __future__ import annotations

from unittest.mock import patch

import pytest

from stt.providers.deepgram import DeepgramProvider


@pytest.mark.asyncio
async def test_deepgram_provider_requires_api_key():
    provider = DeepgramProvider(api_key=None)
    with pytest.raises(RuntimeError, match="DEEPGRAM_API_KEY"):
        await provider.transcribe(b"audio")


@pytest.mark.asyncio
async def test_deepgram_provider_transcribe_extracts_transcript():
    provider = DeepgramProvider(api_key="k")

    class _Resp:
        def to_dict(self):
            return {
                "results": {
                    "channels": [
                        {"alternatives": [{"transcript": "hello from deepgram"}]}
                    ]
                }
            }

    class _Client:
        class listen:
            class prerecorded:
                @staticmethod
                def v(_version):
                    class _V:
                        @staticmethod
                        def transcribe_file(_payload, _options):
                            return _Resp()

                    return _V()

    with patch("deepgram.DeepgramClient", return_value=_Client()):
        text = await provider.transcribe(b"audio", language="en")
    assert text == "hello from deepgram"
