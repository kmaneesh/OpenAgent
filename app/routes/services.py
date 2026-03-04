"""Services route — GET /services, POST /services/{name}/restart"""

from __future__ import annotations

import json
from pathlib import Path

from fastapi import APIRouter, Request
from fastapi.responses import HTMLResponse
from fastapi.templating import Jinja2Templates

router = APIRouter()
templates: Jinja2Templates  # injected by main.py


def _get_services(root: Path) -> list[dict]:
    services_dir = root / "services"
    result = []
    if not services_dir.exists():
        return result
    for manifest in sorted(services_dir.glob("*/service.json")):
        try:
            data = json.loads(manifest.read_text())
            result.append({
                "name": data.get("name", manifest.parent.name),
                "description": data.get("description", ""),
                "version": data.get("version", "?"),
                "socket": data.get("socket", ""),
                "status": "stopped",      # ServiceManager not built yet
                "uptime": None,
                "tools": data.get("tools", []),
            })
        except Exception as exc:
            result.append({
                "name": manifest.parent.name,
                "description": f"Error reading manifest: {exc}",
                "version": "?",
                "socket": "",
                "status": "error",
                "uptime": None,
                "tools": [],
            })
    return result


@router.get("/services")
async def services_page(request: Request):
    return templates.TemplateResponse("services.html", {
        "request": request,
        "active": "services",
        "services": _get_services(request.app.state.root),
    })


@router.post("/services/{name}/restart", response_class=HTMLResponse)
async def restart_service(name: str, request: Request):
    """Stub restart — ServiceManager not built yet. Returns an HTMX partial."""
    # TODO: call ServiceManager.restart(name) when built
    return HTMLResponse(
        f'<span class="text-yellow-400 text-sm">Restart queued for <strong>{name}</strong>'
        f" — ServiceManager not yet available.</span>"
    )
