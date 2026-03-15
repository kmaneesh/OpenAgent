"""Tests for HeartbeatService lifecycle — start/stop and periodic loop.

The existing test_heartbeat_service.py covers tick() and _poll_service().
This file focuses on the background task lifecycle.
"""

from __future__ import annotations

import asyncio
import json
from pathlib import Path
from unittest.mock import AsyncMock, patch, MagicMock

import pytest

from openagent.heartbeat import HeartbeatService


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _make_hb(tmp_path: Path, interval_s: float = 60.0, enabled: bool = True) -> HeartbeatService:
    cfg_dir = tmp_path / "config"
    cfg_dir.mkdir(parents=True, exist_ok=True)
    (cfg_dir / "openagent.yaml").write_text(
        "provider:\n  kind: openai_compat\n  model: test\n", encoding="utf-8"
    )
    return HeartbeatService(
        root=tmp_path,
        interval_s=interval_s,
        enabled=enabled,
        provider_config_path=cfg_dir / "openagent.yaml",
    )


# ---------------------------------------------------------------------------
# Disabled
# ---------------------------------------------------------------------------


class TestDisabled:
    @pytest.mark.asyncio
    async def test_start_does_not_launch_task_when_disabled(self, tmp_path: Path):
        hb = _make_hb(tmp_path, enabled=False)
        await hb.start()
        assert hb._task is None
        assert not hb._running

    @pytest.mark.asyncio
    async def test_stop_is_safe_when_never_started(self, tmp_path: Path):
        hb = _make_hb(tmp_path)
        await hb.stop()  # must not raise

    @pytest.mark.asyncio
    async def test_last_snapshot_is_none_before_start(self, tmp_path: Path):
        hb = _make_hb(tmp_path)
        assert hb.last_snapshot is None


# ---------------------------------------------------------------------------
# Start / Stop
# ---------------------------------------------------------------------------


class TestStartStop:
    @pytest.mark.asyncio
    async def test_start_creates_task_and_runs_initial_tick(self, tmp_path: Path):
        hb = _make_hb(tmp_path, interval_s=60.0)
        with patch.object(hb, "tick", new_callable=AsyncMock) as mock_tick:
            mock_tick.return_value = None
            await hb.start()
            try:
                assert hb._running
                assert hb._task is not None
                assert not hb._task.done()
                # start() calls tick() once immediately
                mock_tick.assert_called_once()
            finally:
                await hb.stop()

    @pytest.mark.asyncio
    async def test_stop_cancels_task(self, tmp_path: Path):
        hb = _make_hb(tmp_path, interval_s=60.0)
        with patch.object(hb, "tick", new_callable=AsyncMock) as mock_tick:
            mock_tick.return_value = None
            await hb.start()
            await hb.stop()
            assert not hb._running
            assert hb._task is None

    @pytest.mark.asyncio
    async def test_double_start_is_idempotent(self, tmp_path: Path):
        hb = _make_hb(tmp_path, interval_s=60.0)
        with patch.object(hb, "tick", new_callable=AsyncMock) as mock_tick:
            mock_tick.return_value = None
            await hb.start()
            task1 = hb._task
            await hb.start()  # second call should be a no-op
            task2 = hb._task
            assert task1 is task2
            await hb.stop()

    @pytest.mark.asyncio
    async def test_double_stop_is_safe(self, tmp_path: Path):
        hb = _make_hb(tmp_path, interval_s=60.0)
        with patch.object(hb, "tick", new_callable=AsyncMock) as mock_tick:
            mock_tick.return_value = None
            await hb.start()
            await hb.stop()
            await hb.stop()  # must not raise


# ---------------------------------------------------------------------------
# Periodic firing
# ---------------------------------------------------------------------------


class TestPeriodic:
    @pytest.mark.asyncio
    async def test_loop_fires_after_interval(self, tmp_path: Path):
        """Verify _run_loop calls tick() when the sleep resolves."""
        tick_count = 0
        sleep_count = 0

        original_sleep = asyncio.sleep

        async def fast_sleep(delay, *args, **kwargs):
            nonlocal sleep_count
            sleep_count += 1
            # Yield to event loop but don't actually wait
            await original_sleep(0)

        async def fake_tick():
            nonlocal tick_count
            tick_count += 1
            if tick_count >= 5:
                hb._running = False  # stop after 5 ticks

        hb = _make_hb(tmp_path, interval_s=999)
        with patch("openagent.heartbeat.service.asyncio.sleep", fast_sleep):
            hb.tick = fake_tick  # type: ignore[method-assign]
            hb._running = True
            hb._tick = 0
            # Run the loop directly for a few iterations
            task = asyncio.create_task(hb._run_loop())
            await asyncio.sleep(0)  # let the task start
            # Wait for loop to self-terminate after 5 ticks
            try:
                await asyncio.wait_for(task, timeout=2.0)
            except asyncio.TimeoutError:
                task.cancel()

        assert tick_count >= 4

    @pytest.mark.asyncio
    async def test_loop_continues_after_tick_exception(self, tmp_path: Path):
        """A failing tick should not crash the loop."""
        tick_count = 0
        original_sleep = asyncio.sleep

        async def fast_sleep(delay, *args, **kwargs):
            await original_sleep(0)

        async def flaky_tick():
            nonlocal tick_count
            tick_count += 1
            if tick_count >= 6:
                hb._running = False
            if tick_count == 2:
                raise RuntimeError("transient error")

        hb = _make_hb(tmp_path, interval_s=999)
        with patch("openagent.heartbeat.service.asyncio.sleep", fast_sleep):
            hb.tick = flaky_tick  # type: ignore[method-assign]
            hb._running = True
            hb._tick = 0
            task = asyncio.create_task(hb._run_loop())
            await asyncio.sleep(0)
            try:
                await asyncio.wait_for(task, timeout=2.0)
            except asyncio.TimeoutError:
                task.cancel()

        # Loop should continue after the error on tick 2
        assert tick_count >= 4


# ---------------------------------------------------------------------------
# Interval clamping
# ---------------------------------------------------------------------------


class TestIntervalClamping:
    def test_interval_clamped_to_minimum_one_second(self, tmp_path: Path):
        hb = _make_hb(tmp_path, interval_s=0.0)
        assert hb.interval_s == 1.0

    def test_negative_interval_clamped(self, tmp_path: Path):
        hb = _make_hb(tmp_path, interval_s=-10.0)
        assert hb.interval_s == 1.0

    def test_valid_interval_preserved(self, tmp_path: Path):
        hb = _make_hb(tmp_path, interval_s=30.0)
        assert hb.interval_s == 30.0
