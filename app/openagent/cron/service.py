"""Cron service for scheduling agent tasks via the message bus."""

from __future__ import annotations

import asyncio
import json
import time
import uuid
from datetime import datetime
from pathlib import Path
from typing import Any, Callable, Coroutine

from openagent.observability import get_logger, log_event
from openagent.cron.types import CronJob, CronJobState, CronPayload, CronSchedule

logger = get_logger(__name__)


def _now_ms() -> int:
    return int(time.time() * 1000)


def _compute_next_run(schedule: CronSchedule, now_ms: int) -> int | None:
    if schedule.kind == "at":
        return schedule.at_ms if schedule.at_ms and schedule.at_ms > now_ms else None

    if schedule.kind == "every":
        if not schedule.every_ms or schedule.every_ms <= 0:
            return None
        return now_ms + schedule.every_ms

    if schedule.kind == "cron" and schedule.expr:
        try:
            from zoneinfo import ZoneInfo
            from croniter import croniter

            base_time = now_ms / 1000
            tz = ZoneInfo(schedule.tz) if schedule.tz else datetime.now().astimezone().tzinfo
            base_dt = datetime.fromtimestamp(base_time, tz=tz)
            cron = croniter(schedule.expr, base_dt)
            next_dt = cron.get_next(datetime)
            return int(next_dt.timestamp() * 1000)
        except Exception as exc:
            log_event(
                logger,
                40,
                f"failed to parse cron expr: {exc}",
                component="cron",
                operation="compute_run",
                error=str(exc)
            )
            return None

    return None


class CronService:
    def __init__(
        self,
        store_path: Path,
        on_job: Callable[[CronJob], Coroutine[Any, Any, str | None]] | None = None
    ):
        self.store_path = store_path
        self.on_job = on_job
        self._jobs: list[CronJob] = []
        self._last_mtime: float = 0.0
        self._timer_task: asyncio.Task[None] | None = None
        self._running = False

    def _load_store(self) -> None:
        if self.store_path.exists():
            mtime = self.store_path.stat().st_mtime
            if self._jobs and mtime == self._last_mtime:
                return

        if self.store_path.exists():
            try:
                data = json.loads(self.store_path.read_text(encoding="utf-8"))
                jobs = []
                for j in data.get("jobs", []):
                    jobs.append(CronJob(
                        id=j["id"],
                        name=j["name"],
                        enabled=j.get("enabled", True),
                        schedule=CronSchedule(
                            kind=j["schedule"]["kind"],
                            at_ms=j["schedule"].get("atMs"),
                            every_ms=j["schedule"].get("everyMs"),
                            expr=j["schedule"].get("expr"),
                            tz=j["schedule"].get("tz"),
                        ),
                        payload=CronPayload(
                            kind=j["payload"].get("kind", "agent_turn"),
                            message=j["payload"].get("message", ""),
                            deliver=j["payload"].get("deliver", False),
                            channel=j["payload"].get("channel"),
                            to=j["payload"].get("to"),
                        ),
                        state=CronJobState(
                            next_run_at_ms=j.get("state", {}).get("nextRunAtMs"),
                            last_run_at_ms=j.get("state", {}).get("lastRunAtMs"),
                            last_status=j.get("state", {}).get("lastStatus"),
                            last_error=j.get("state", {}).get("lastError"),
                        ),
                        created_at_ms=j.get("createdAtMs", 0),
                        updated_at_ms=j.get("updatedAtMs", 0),
                        delete_after_run=j.get("deleteAfterRun", False),
                    ))
                self._jobs = jobs
            except Exception as e:
                log_event(
                    logger,
                    30,
                    f"failed to load cron store: {e}",
                    component="cron",
                    operation="load",
                    error=str(e)
                )
                if not self._jobs:
                    self._jobs = []
        else:
            self._jobs = []

    def _save_store(self) -> None:
        self.store_path.parent.mkdir(parents=True, exist_ok=True)
        data = {
            "version": 1,
            "jobs": [
                {
                    "id": j.id,
                    "name": j.name,
                    "enabled": j.enabled,
                    "schedule": {
                        "kind": j.schedule.kind,
                        "atMs": j.schedule.at_ms,
                        "everyMs": j.schedule.every_ms,
                        "expr": j.schedule.expr,
                        "tz": j.schedule.tz,
                    },
                    "payload": {
                        "kind": j.payload.kind,
                        "message": j.payload.message,
                        "deliver": j.payload.deliver,
                        "channel": j.payload.channel,
                        "to": j.payload.to,
                    },
                    "state": {
                        "nextRunAtMs": j.state.next_run_at_ms,
                        "lastRunAtMs": j.state.last_run_at_ms,
                        "lastStatus": j.state.last_status,
                        "lastError": j.state.last_error,
                    },
                    "createdAtMs": j.created_at_ms,
                    "updatedAtMs": j.updated_at_ms,
                    "deleteAfterRun": j.delete_after_run,
                }
                for j in self._jobs
            ]
        }
        self.store_path.write_text(json.dumps(data, indent=2, ensure_ascii=False), encoding="utf-8")
        self._last_mtime = self.store_path.stat().st_mtime

    async def start(self) -> None:
        if self._running:
            return
        self._running = True
        self._load_store()
        self._recompute_next_runs()
        self._save_store()
        self._arm_timer()
        log_event(
            logger,
            20,
            "cron started",
            component="cron",
            operation="start",
            jobs=len(self._jobs)
        )

    async def stop(self) -> None:
        self._running = False
        if self._timer_task:
            self._timer_task.cancel()
            try:
                await self._timer_task
            except asyncio.CancelledError:
                pass
            self._timer_task = None
        log_event(logger, 20, "cron stopped", component="cron", operation="stop")

    def _recompute_next_runs(self) -> None:
        now = _now_ms()
        for job in self._jobs:
            if job.enabled:
                job.state.next_run_at_ms = _compute_next_run(job.schedule, now)

    def _get_next_wake_ms(self) -> int | None:
        times = [j.state.next_run_at_ms for j in self._jobs if j.enabled and j.state.next_run_at_ms]
        return min(times) if times else None

    def _arm_timer(self) -> None:
        if self._timer_task:
            self._timer_task.cancel()

        next_wake = self._get_next_wake_ms()
        if not next_wake or not self._running:
            return

        delay_s = max(0.01, (next_wake - _now_ms()) / 1000.0)

        async def tick() -> None:
            try:
                await asyncio.sleep(delay_s)
                if self._running:
                    await self._on_timer()
            except asyncio.CancelledError:
                pass

        self._timer_task = asyncio.create_task(tick(), name="cron.tick")

    async def _on_timer(self) -> None:
        self._load_store()
        now = _now_ms()
        due_jobs = [j for j in self._jobs if j.enabled and j.state.next_run_at_ms and now >= j.state.next_run_at_ms]

        for job in due_jobs:
            await self._execute_job(job)

        self._save_store()
        self._arm_timer()

    async def _execute_job(self, job: CronJob) -> None:
        start_ms = _now_ms()
        log_event(
            logger,
            20,
            f"executing job '{job.name}'",
            component="cron",
            operation="execute_job",
            job_id=job.id
        )

        try:
            if self.on_job:
                await self.on_job(job)
            job.state.last_status = "ok"
            job.state.last_error = None
        except Exception as e:
            job.state.last_status = "error"
            job.state.last_error = str(e)
            log_event(
                logger,
                40,
                f"job failed: {e}",
                component="cron",
                operation="execute_job",
                job_id=job.id,
                error=str(e)
            )

        job.state.last_run_at_ms = start_ms
        job.updated_at_ms = _now_ms()

        if job.schedule.kind == "at":
            if job.delete_after_run:
                self._jobs = [j for j in self._jobs if j.id != job.id]
            else:
                job.enabled = False
                job.state.next_run_at_ms = None
        else:
            job.state.next_run_at_ms = _compute_next_run(job.schedule, _now_ms())
