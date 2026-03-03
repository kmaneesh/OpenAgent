"""Local Faster-Whisper STT provider (default)."""

from __future__ import annotations

import asyncio
import os
import tempfile
from pathlib import Path
from typing import Any

from .base import STTProvider


class FasterWhisperProvider(STTProvider):
    def __init__(
        self,
        *,
        model_size: str = "small",
        device: str = "auto",
        compute_type: str = "int8",
    ):
        self.model_size = model_size
        self.device = device
        self.compute_type = compute_type
        self._model: Any | None = None

    async def transcribe(self, audio_data: bytes, **kwargs) -> str:
        model = await self._get_model()
        suffix = str(kwargs.get("file_suffix", ".wav"))
        path = await self._write_temp_audio(audio_data, suffix=suffix)
        vad_filter = bool(kwargs.get("vad_filter", True))
        language = kwargs.get("language")

        def _run_transcription() -> str:
            segments, _info = model.transcribe(
                str(path),
                vad_filter=vad_filter,
                language=language,
                beam_size=1,
            )
            return " ".join(seg.text.strip() for seg in segments if getattr(seg, "text", "").strip())

        try:
            return (await asyncio.to_thread(_run_transcription)).strip()
        finally:
            await asyncio.to_thread(self._safe_remove, path)

    async def _get_model(self):
        if self._model is not None:
            return self._model

        def _load():
            from faster_whisper import WhisperModel

            return WhisperModel(
                self.model_size,
                device=self.device,
                compute_type=self.compute_type,
            )

        self._model = await asyncio.to_thread(_load)
        return self._model

    async def _write_temp_audio(self, audio_data: bytes, *, suffix: str) -> Path:
        def _write() -> Path:
            fd, tmp_path = tempfile.mkstemp(prefix="openagent-stt-", suffix=suffix)
            os.close(fd)
            path = Path(tmp_path)
            path.write_bytes(audio_data)
            return path

        return await asyncio.to_thread(_write)

    @staticmethod
    def _safe_remove(path: Path) -> None:
        try:
            path.unlink(missing_ok=True)
        except Exception:
            pass
