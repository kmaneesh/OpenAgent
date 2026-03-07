"""Dashboard route — GET /"""

from __future__ import annotations

import importlib.metadata
import time
from typing import Any

import psutil
from fastapi import APIRouter, Request
from fastapi.templating import Jinja2Templates

router = APIRouter()
templates: Jinja2Templates  # injected by main.py

_START_TIME = time.time()


def _online_services_from_manager(mgr) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    """Get Go and Rust services from ServiceManager, filtered to online (running) only."""
    go_services: list[dict[str, Any]] = []
    rust_services: list[dict[str, Any]] = []

    for svc in mgr.list_services():
        d = svc.to_dict()
        if d.get("status") != "running":
            continue
        entry = {
            "name": d["name"],
            "version": d.get("version", "?"),
            "status": "online",
        }
        if d.get("runtime") == "rust":
            rust_services.append(entry)
        else:
            go_services.append(entry)

    return go_services, rust_services


def _system_stats() -> dict:
    cpu = psutil.cpu_percent(interval=0.1)
    ram = psutil.virtual_memory()
    disk = psutil.disk_usage("/")
    temp: float | None = None
    try:
        temps = psutil.sensors_temperatures()
        if temps:
            for entries in temps.values():
                if entries:
                    temp = entries[0].current
                    break
    except AttributeError:
        pass  # Windows / platforms without sensor support

    uptime_s = int(time.time() - _START_TIME)
    h, rem = divmod(uptime_s, 3600)
    m, s = divmod(rem, 60)

    return {
        "cpu_pct": cpu,
        "ram_pct": ram.percent,
        "ram_used_mb": ram.used // (1024 * 1024),
        "ram_total_mb": ram.total // (1024 * 1024),
        "disk_pct": disk.percent,
        "disk_used_gb": disk.used // (1024 ** 3),
        "disk_total_gb": disk.total // (1024 ** 3),
        "temp_c": round(temp, 1) if temp is not None else None,
        "uptime": f"{h:02d}:{m:02d}:{s:02d}",
    }


def _installed_extensions() -> list[dict]:
    eps = importlib.metadata.entry_points(group="openagent.extensions")
    result = []
    for ep in eps:
        try:
            dist = importlib.metadata.distribution(ep.value.split(":")[0].split(".")[0])
            version = dist.metadata["Version"]
        except Exception:
            version = "?"
        result.append({"name": ep.name, "entry": ep.value, "version": version, "status": "installed"})
    return result


@router.get("/")
async def dashboard(request: Request):
    mgr = getattr(request.app.state, "service_manager", None)
    if mgr:
        services, rust_services = _online_services_from_manager(mgr)
    else:
        services = []
        rust_services = []

    return templates.TemplateResponse("dashboard.html", {
        "request": request,
        "active": "dashboard",
        "stats": _system_stats(),
        "extensions": _installed_extensions(),
        "services": services,
        "rust_services": rust_services,
    })


@router.get("/api/stats")
async def stats_partial(request: Request):
    """Partial for HTMX stat-card polling — returns cards only, no layout."""
    mgr = getattr(request.app.state, "service_manager", None)
    if mgr:
        services, rust_services = _online_services_from_manager(mgr)
    else:
        services = []
        rust_services = []

    return templates.TemplateResponse("_stats_cards.html", {
        "request": request,
        "stats": _system_stats(),
        "extensions": _installed_extensions(),
        "services": services,
        "rust_services": rust_services,
    })
