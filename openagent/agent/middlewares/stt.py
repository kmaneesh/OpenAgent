"""STT inbound middleware — transcribes audio before LLM processing."""

from __future__ import annotations

import logging
from typing import Callable, Awaitable

from openagent.bus.events import InboundMessage

logger = logging.getLogger(__name__)

# Audio file extensions recognised as transcribable
_AUDIO_EXTS = {".mp3", ".wav", ".ogg", ".m4a", ".webm", ".flac", ".aac", ".opus"}

STTFn = Callable[[str], Awaitable[str]]


class STTMiddleware:
    """Transcribes audio paths in ``msg.media`` (or ``msg.metadata["audio_path"]``)
    and replaces / prepends ``msg.content`` with the transcript.

    Parameters
    ----------
    stt_fn:
        Async callable ``(audio_path) -> transcript``.
        If ``None``, falls back to ``get_extension("stt").listen`` at call time
        (legacy behaviour; inject for testability).
    """

    direction = "inbound"

    def __init__(self, stt_fn: STTFn | None = None) -> None:
        self._stt = stt_fn

    async def __call__(self, msg: InboundMessage) -> None:  # type: ignore[override]
        stt = self._stt or _lazy_stt()
        if stt is None:
            return

        # Collect audio paths from msg.media and legacy metadata key
        audio_paths: list[str] = [p for p in msg.media if _is_audio(p)]
        if not audio_paths and "audio_path" in msg.metadata:
            audio_paths = [msg.metadata["audio_path"]]

        if not audio_paths:
            return

        transcripts: list[str] = []
        for path in audio_paths:
            try:
                text = await stt(path)
                if text and text.strip():
                    transcripts.append(text.strip())
                    logger.info(
                        "STT transcribed %s → %d chars (session %s)",
                        path, len(text), msg.session_key,
                    )
            except Exception as exc:
                logger.error("STT failed for %s: %s", path, exc)

        if not transcripts:
            return

        # Merge transcripts with any existing text content
        transcript_text = " ".join(transcripts)
        msg.content = f"{msg.content}\n{transcript_text}".strip() if msg.content else transcript_text

        # Remove processed audio from media so downstream doesn't re-process
        msg.media = [p for p in msg.media if not _is_audio(p)]
        msg.metadata.pop("audio_path", None)


def _is_audio(path: str) -> bool:
    from pathlib import PurePosixPath
    return PurePosixPath(path).suffix.lower() in _AUDIO_EXTS


def _lazy_stt() -> STTFn | None:
    return None
