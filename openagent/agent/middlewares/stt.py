"""Middleware for intercepting and transcribing audio inputs."""

import logging
from openagent.bus.events import InboundMessage
from . import NextCall

logger = logging.getLogger(__name__)

class STTMiddleware:
    """Intersects messages containing audio metadata and transcribes them before LLM processing."""
    
    async def __call__(self, msg: InboundMessage, next_call: NextCall) -> None:
        if msg.metadata and "audio_path" in msg.metadata:
            audio_path = msg.metadata["audio_path"]
            from openagent.manager import get_extension
            stt_ext = get_extension("stt")
            if stt_ext:
                logger.info("Transcribing audio for session %s at %s", msg.session_key, audio_path)
                try:
                    transcript = await stt_ext.listen(file=audio_path)
                    if transcript:
                        prefix = f"{msg.content}\n" if msg.content else ""
                        msg.content = f"{prefix}[Audio Transcribed]: {transcript}"
                except Exception as exc:
                    logger.error("STT transcription failed: %s", exc)
                    
        await next_call(msg)
