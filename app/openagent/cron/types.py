"""Data models and types for the OpenAgent Cron service."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Literal

@dataclass(slots=True)
class CronSchedule:
    kind: Literal["cron", "every", "at"]
    expr: str | None = None
    every_ms: int | None = None
    at_ms: int | None = None
    tz: str | None = None

@dataclass(slots=True)
class CronPayload:
    kind: str = "agent_turn"
    message: str = ""
    deliver: bool = False
    channel: str | None = None
    to: str | None = None

@dataclass(slots=True)
class CronJobState:
    next_run_at_ms: int | None = None
    last_run_at_ms: int | None = None
    last_status: str | None = None
    last_error: str | None = None

@dataclass(slots=True)
class CronJob:
    id: str
    name: str
    schedule: CronSchedule
    payload: CronPayload
    enabled: bool = True
    delete_after_run: bool = False
    state: CronJobState = field(default_factory=CronJobState)
    created_at_ms: int = 0
    updated_at_ms: int = 0
