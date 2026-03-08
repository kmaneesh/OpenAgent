"""End-to-end voice message test.

Injects a real audio file into the full agent pipeline:

    audio file
        → STTMiddleware (faster-whisper tiny)
        → AgentLoop (real LLM from config/openagent.yaml, or mock)
        → OutboundMessage printed to stdout

Usage
-----
# With a real WhatsApp OGG voice note and live LLM:
    STT_TEST_FILE=data/artifacts/whatsapp/voice.ogg \\
        .venv/bin/pytest extensions/stt/tests/test_e2e_voice.py -v -s -m integration

# With mock LLM (no API key needed, fast):
    STT_TEST_FILE=data/artifacts/whatsapp/voice.ogg STT_MOCK_LLM=1 \\
        .venv/bin/pytest extensions/stt/tests/test_e2e_voice.py -v -s -m integration

When STT_TEST_FILE is not set the test is skipped.
STT_MOCK_LLM=1 replaces the LLM with a stub that echoes the transcript back.
"""

from __future__ import annotations

import asyncio
import os
from pathlib import Path

import pytest

from openagent.agent.loop import AgentLoop
from openagent.agent.middlewares.stt import STTMiddleware
from openagent.agent.middlewares.tts import TTSMiddleware
from openagent.agent.tools import ToolRegistry
from openagent.bus.bus import MessageBus
from openagent.bus.events import InboundMessage, SenderInfo
from openagent.session import SessionManager, SqliteSessionBackend


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _audio_path() -> Path | None:
    p = os.getenv("STT_TEST_FILE")
    return Path(p) if p else None


def _use_mock_llm() -> bool:
    return os.getenv("STT_MOCK_LLM", "").strip() not in ("", "0", "false")


class _EchoProvider:
    """Minimal LLM provider stub — echoes the last user message back."""

    async def chat(self, messages, *, tools=None, **_):
        from openagent.providers.base import LLMResponse
        last = next(
            (m.content for m in reversed(messages) if getattr(m, "role", None) == "user"),
            "[no user message]",
        )
        return LLMResponse(content=f"[echo] {last}", tool_calls=[])


async def _build_stack(tmp_path: Path):
    """Boot a minimal real agent stack and return (bus, loop, stt_ext)."""
    from openagent.config import load_config
    from stt.plugin import STTExtension

    from tts.plugin import TTSExtension

    ROOT = Path(__file__).resolve().parents[4]  # repo root
    cfg = load_config(ROOT / "config" / "openagent.yaml")

    # STT extension — model from config/openagent.yaml stt section
    stt_ext = STTExtension(config={
        "provider": cfg.stt.provider,
        "whisper_model": cfg.stt.whisper_model,
    })
    await stt_ext.initialize()

    # TTS extension — voice/provider from config/openagent.yaml tts section
    tts_ext = TTSExtension(config={
        "provider": cfg.tts.provider,
        "voice": cfg.tts.voice,
        "speed": cfg.tts.speed,
        "volume": cfg.tts.volume,
        "api_key": cfg.tts.api_key,
        "group_id": cfg.tts.group_id,
    })
    await tts_ext.initialize()

    # Message bus
    bus = MessageBus()
    await bus.start()

    # Session manager — SQLite in a temp dir
    db = tmp_path / "e2e_sessions.db"
    backend = SqliteSessionBackend(db)
    sessions = SessionManager(backend=backend, summarise_after=0)
    await sessions.start()

    # Provider — real from config or echo stub
    if _use_mock_llm():
        provider = _EchoProvider()
    else:
        from openagent.providers import get_provider
        provider = get_provider(cfg.provider)

    # Tool registry (no Go services in test)
    from openagent.services.manager import ServiceManager
    svc_mgr = ServiceManager(root=Path("."))
    tools = ToolRegistry(svc_mgr)

    async def _stt_fn(audio_path: str) -> str:
        return await stt_ext.listen(file=audio_path)

    async def _tts_fn(text: str) -> str:
        # Run TTS — write audio to artifacts dir so callers can inspect it.
        # Returns empty string (agent loop doesn't use the return value).
        audio = await tts_ext.speak(text)
        out = ROOT / "data" / "artifacts" / "tts_reply.mp3"
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_bytes(audio)
        print(f"[e2e] TTS audio written → {out}")
        return ""

    agent_cfg = cfg.default_agent
    agent = AgentLoop(
        bus=bus,
        provider=provider,
        sessions=sessions,
        tools=tools,
        system_prompt=agent_cfg.system_prompt,
        max_iterations=agent_cfg.max_iterations,
        max_tool_output=agent_cfg.max_tool_output,
        middlewares=[
            STTMiddleware(stt_fn=_stt_fn),
            TTSMiddleware(tts_fn=_tts_fn),
        ],
    )
    await agent.start()

    return bus, agent, sessions, stt_ext, tts_ext


# ---------------------------------------------------------------------------
# Test
# ---------------------------------------------------------------------------

@pytest.mark.integration
@pytest.mark.asyncio
async def test_voice_message_through_agent(tmp_path: Path):
    """
    Injects a real audio file as a WhatsApp voice note and asserts that the
    agent produces a non-empty text reply.

    Set STT_TEST_FILE=/path/to/audio.ogg before running.
    Set STT_MOCK_LLM=1 to use an echo stub instead of the configured LLM.
    """
    audio = _audio_path()
    if audio is None:
        pytest.skip("Set STT_TEST_FILE=/path/to/audio.ogg to run this test")
    if not audio.exists():
        pytest.fail(f"STT_TEST_FILE does not exist: {audio}")

    bus, agent, sessions, stt_ext, tts_ext = await _build_stack(tmp_path)

    try:
        # Inject the audio as a WhatsApp inbound message
        msg = InboundMessage(
            platform="whatsapp",
            channel_id="test-chat",
            sender=SenderInfo(platform="whatsapp", user_id="test-user"),
            content="[Voice message]",   # placeholder; STT will replace this
            media=[str(audio)],
        )

        print(f"\n[e2e] Injecting audio: {audio}")
        await bus.publish(msg)

        # Wait for the agent's reply on the outbound queue (timeout = 60 s for slow GPUs)
        try:
            reply = await asyncio.wait_for(bus.outbound.get(), timeout=60.0)
        except asyncio.TimeoutError:
            pytest.fail("Agent did not reply within 60 s — check LLM connectivity")

        assert reply is not None, "Expected an OutboundMessage, got None sentinel"
        assert reply.content and reply.content.strip(), "Agent reply must not be empty"

        print(f"[e2e] STT transcript injected via media path: {audio.name}")
        print(f"[e2e] Agent reply:\n  {reply.content}")

    finally:
        await agent.stop()
        await sessions.stop()
        await bus.close()
        await stt_ext.shutdown()
        await tts_ext.shutdown()
