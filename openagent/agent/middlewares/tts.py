"""TTS outbound middleware — speaks the LLM reply and attaches audio to outbound.media."""

from __future__ import annotations

import logging
from typing import Callable, Awaitable

from openagent.bus.events import OutboundMessage

logger = logging.getLogger(__name__)

TTSFn = Callable[[str], Awaitable[str]]


class TTSMiddleware:
    """Converts ``outbound.content`` to audio and appends the path to ``outbound.media``.

    Platform adapters that support audio (Telegram voice, WhatsApp audio) should
    check ``outbound.media`` for audio paths and deliver them alongside the text.
    Platforms that don't support audio simply ignore ``media``.

    Parameters
    ----------
    tts_fn:
        Async callable ``(text) -> audio_path``.
        If ``None``, falls back to ``get_extension("tts").speak`` at call time.
    min_chars:
        Skip TTS synthesis for replies shorter than this (e.g. "ok", "yes").
        Defaults to 10.
    """

    direction = "outbound"

    def __init__(self, tts_fn: TTSFn | None = None, *, min_chars: int = 10) -> None:
        self._tts = tts_fn
        self._min_chars = min_chars

    async def __call__(self, msg: OutboundMessage) -> None:  # type: ignore[override]
        # Skip if already has audio, content is too short, or it's a streaming chunk
        if msg.metadata.get("stream_chunk") and not msg.metadata.get("stream_end"):
            return
        if not msg.content or len(msg.content) < self._min_chars:
            return
        if any(_is_audio(p) for p in msg.media):
            return  # audio already attached upstream

        tts = self._tts or _lazy_tts()
        if tts is None:
            return

        try:
            audio_path = await tts(msg.content)
            if audio_path:
                msg.media.append(audio_path)
                logger.info(
                    "TTS synthesised %d chars → %s (session %s)",
                    len(msg.content), audio_path, msg.session_key,
                )
        except Exception as exc:
            logger.error("TTS failed: %s", exc)


_AUDIO_EXTS = {".mp3", ".wav", ".ogg", ".m4a", ".webm", ".flac", ".aac", ".opus"}


def _is_audio(path: str) -> bool:
    from pathlib import PurePosixPath
    return PurePosixPath(path).suffix.lower() in _AUDIO_EXTS


def _lazy_tts() -> TTSFn | None:
    """Resolve TTS extension at call time (fallback when tts_fn not injected)."""
    try:
        from openagent.manager import get_extension
        ext = get_extension("tts")
        if ext is not None:
            return ext.speak
    except Exception:
        pass
    return None
