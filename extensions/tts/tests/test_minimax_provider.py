from __future__ import annotations

import base64
import json

import pytest

from tts.providers.minimax import MiniMaxProvider


class _FakeContent:
    def __init__(self, chunks):
        self._chunks = chunks

    def __aiter__(self):
        self._iter = iter(self._chunks)
        return self

    async def __anext__(self):
        try:
            return next(self._iter)
        except StopIteration as exc:
            raise StopAsyncIteration from exc


class _FakeResponse:
    def __init__(self, *, chunks=None, json_payload=None):
        self.content = _FakeContent(chunks or [])
        self._json_payload = json_payload or {}

    async def __aenter__(self):
        return self

    async def __aexit__(self, exc_type, exc, tb):
        return False

    def raise_for_status(self):
        return None

    async def json(self):
        return self._json_payload


class _FakeSession:
    def __init__(self, response):
        self._response = response
        self.calls = []

    def post(self, url, **kwargs):
        self.calls.append((url, kwargs))
        return self._response

    async def close(self):
        return None


@pytest.mark.asyncio
async def test_minimax_provider_generate_stream_decodes_stream_chunks():
    payload = base64.b64encode(b"hello").decode("utf-8")
    stream_line = f"data: {json.dumps({'audio': payload})}\n".encode("utf-8")
    response = _FakeResponse(chunks=[stream_line, b"data: [DONE]\n"])
    session = _FakeSession(response)
    provider = MiniMaxProvider(api_key="k", group_id="g", session=session)
    audio = await provider.generate("test", stream=True)
    assert audio == b"hello"


@pytest.mark.asyncio
async def test_minimax_provider_generate_non_streaming_json():
    payload = base64.b64encode(b"world").decode("utf-8")
    response = _FakeResponse(json_payload={"data": {"audio": payload}})
    session = _FakeSession(response)
    provider = MiniMaxProvider(api_key="k", group_id="g", session=session)
    audio = await provider.generate("test", stream=False)
    assert audio == b"world"
