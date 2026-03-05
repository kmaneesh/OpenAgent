"""Tests for openagent.agent.middlewares — STTMiddleware + TTSMiddleware."""

from __future__ import annotations

import pytest
import pytest_asyncio

from openagent.agent.middlewares import AgentMiddleware
from openagent.agent.middlewares.stt import STTMiddleware
from openagent.agent.middlewares.tts import TTSMiddleware
from openagent.bus.events import InboundMessage, OutboundMessage, SenderInfo


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _inbound(content: str = "", media: list[str] | None = None, **meta) -> InboundMessage:
    return InboundMessage(
        platform="test",
        channel_id="chan1",
        sender=SenderInfo(platform="test", user_id="u1"),
        content=content,
        media=media or [],
        metadata=meta,
    )


def _outbound(content: str = "", media: list[str] | None = None, **meta) -> OutboundMessage:
    return OutboundMessage(
        platform="test",
        channel_id="chan1",
        content=content,
        media=media or [],
        metadata=meta,
    )


async def _stt_fn(path: str) -> str:
    return f"transcript of {path}"


async def _tts_fn(text: str) -> str:
    return "/data/artifacts/audio.mp3"


# ---------------------------------------------------------------------------
# Protocol conformance
# ---------------------------------------------------------------------------


class TestProtocol:
    def test_stt_has_direction(self):
        mw = STTMiddleware()
        assert mw.direction == "inbound"

    def test_tts_has_direction(self):
        mw = TTSMiddleware()
        assert mw.direction == "outbound"

    def test_stt_is_callable(self):
        mw = STTMiddleware()
        assert callable(mw)

    def test_tts_is_callable(self):
        mw = TTSMiddleware()
        assert callable(mw)


# ---------------------------------------------------------------------------
# STTMiddleware
# ---------------------------------------------------------------------------


class TestSTTMiddleware:
    @pytest.mark.asyncio
    async def test_transcribes_audio_in_media(self):
        mw = STTMiddleware(stt_fn=_stt_fn)
        msg = _inbound(content="", media=["recording.ogg"])
        await mw(msg)
        assert "transcript of recording.ogg" in msg.content

    @pytest.mark.asyncio
    async def test_prepends_existing_content(self):
        mw = STTMiddleware(stt_fn=_stt_fn)
        msg = _inbound(content="hello", media=["note.mp3"])
        await mw(msg)
        assert "hello" in msg.content
        assert "transcript of note.mp3" in msg.content

    @pytest.mark.asyncio
    async def test_removes_audio_from_media(self):
        mw = STTMiddleware(stt_fn=_stt_fn)
        msg = _inbound(media=["voice.wav", "image.png"])
        await mw(msg)
        assert "image.png" in msg.media
        assert "voice.wav" not in msg.media

    @pytest.mark.asyncio
    async def test_legacy_metadata_key(self):
        mw = STTMiddleware(stt_fn=_stt_fn)
        msg = _inbound(audio_path="legacy.flac")
        await mw(msg)
        assert "transcript of legacy.flac" in msg.content
        assert "audio_path" not in msg.metadata

    @pytest.mark.asyncio
    async def test_no_op_when_no_audio(self):
        mw = STTMiddleware(stt_fn=_stt_fn)
        msg = _inbound(content="text only", media=["image.png"])
        await mw(msg)
        assert msg.content == "text only"

    @pytest.mark.asyncio
    async def test_skips_when_no_stt_fn(self):
        mw = STTMiddleware(stt_fn=None)
        msg = _inbound(media=["audio.mp3"])
        # _lazy_stt will return None since no extension loaded
        await mw(msg)
        assert msg.content == ""

    @pytest.mark.asyncio
    async def test_handles_stt_exception(self):
        async def failing_stt(path: str) -> str:
            raise RuntimeError("STT backend unavailable")

        mw = STTMiddleware(stt_fn=failing_stt)
        msg = _inbound(media=["audio.mp3"])
        await mw(msg)  # must not raise
        assert msg.content == ""

    @pytest.mark.asyncio
    async def test_multiple_audio_files(self):
        mw = STTMiddleware(stt_fn=_stt_fn)
        msg = _inbound(media=["a.mp3", "b.wav"])
        await mw(msg)
        assert "transcript of a.mp3" in msg.content
        assert "transcript of b.wav" in msg.content
        assert msg.media == []

    @pytest.mark.asyncio
    async def test_non_audio_extensions_ignored(self):
        mw = STTMiddleware(stt_fn=_stt_fn)
        msg = _inbound(media=["photo.jpg", "doc.pdf"])
        await mw(msg)
        assert msg.content == ""
        assert "photo.jpg" in msg.media


# ---------------------------------------------------------------------------
# TTSMiddleware
# ---------------------------------------------------------------------------


class TestTTSMiddleware:
    @pytest.mark.asyncio
    async def test_synthesises_reply(self):
        mw = TTSMiddleware(tts_fn=_tts_fn)
        msg = _outbound(content="Hello, how can I help you today?")
        await mw(msg)
        assert "/data/artifacts/audio.mp3" in msg.media

    @pytest.mark.asyncio
    async def test_skips_short_content(self):
        mw = TTSMiddleware(tts_fn=_tts_fn, min_chars=10)
        msg = _outbound(content="ok")
        await mw(msg)
        assert msg.media == []

    @pytest.mark.asyncio
    async def test_skips_streaming_chunk(self):
        mw = TTSMiddleware(tts_fn=_tts_fn)
        msg = _outbound(content="streaming partial text...", stream_chunk=True)
        await mw(msg)
        assert msg.media == []

    @pytest.mark.asyncio
    async def test_runs_on_stream_end(self):
        mw = TTSMiddleware(tts_fn=_tts_fn)
        msg = _outbound(content="Full accumulated text response.", stream_chunk=True, stream_end=True)
        await mw(msg)
        assert "/data/artifacts/audio.mp3" in msg.media

    @pytest.mark.asyncio
    async def test_skips_if_audio_already_attached(self):
        call_count = 0

        async def counting_tts(text: str) -> str:
            nonlocal call_count
            call_count += 1
            return "/audio.mp3"

        mw = TTSMiddleware(tts_fn=counting_tts)
        msg = _outbound(content="Some long reply for the user here", media=["/existing.wav"])
        await mw(msg)
        assert call_count == 0

    @pytest.mark.asyncio
    async def test_skips_when_no_tts_fn(self):
        mw = TTSMiddleware(tts_fn=None)
        msg = _outbound(content="Some reply to synthesise now")
        await mw(msg)
        assert msg.media == []

    @pytest.mark.asyncio
    async def test_handles_tts_exception(self):
        async def failing_tts(text: str) -> str:
            raise RuntimeError("TTS backend unavailable")

        mw = TTSMiddleware(tts_fn=failing_tts)
        msg = _outbound(content="This will fail to synthesise audio")
        await mw(msg)  # must not raise
        assert msg.media == []

    @pytest.mark.asyncio
    async def test_empty_content_skipped(self):
        called = False

        async def spy_tts(text: str) -> str:
            nonlocal called
            called = True
            return "/audio.mp3"

        mw = TTSMiddleware(tts_fn=spy_tts)
        msg = _outbound(content="")
        await mw(msg)
        assert not called


# ---------------------------------------------------------------------------
# Direction chain split (integration-style)
# ---------------------------------------------------------------------------


class TestDirectionSplit:
    def test_middlewares_split_by_direction(self):
        """Verify the loop can partition a mixed list by direction."""
        mws = [STTMiddleware(), TTSMiddleware()]
        inbound = [m for m in mws if m.direction == "inbound"]
        outbound = [m for m in mws if m.direction == "outbound"]
        assert len(inbound) == 1
        assert len(outbound) == 1
        assert isinstance(inbound[0], STTMiddleware)
        assert isinstance(outbound[0], TTSMiddleware)
