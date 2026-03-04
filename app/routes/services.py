"""Services route — GET /services, POST /services/{name}/restart"""

from __future__ import annotations

from fastapi import APIRouter, Request
from fastapi.responses import HTMLResponse
from fastapi.templating import Jinja2Templates

from openagent.services.manager import ServiceManager

router = APIRouter()
templates: Jinja2Templates  # injected by main.py


@router.get("/services")
async def services_page(request: Request):
    mgr: ServiceManager | None = getattr(request.app.state, "service_manager", None)
    service_list = [s.to_dict() for s in mgr.list_services()] if mgr else []
    return templates.TemplateResponse("services.html", {
        "request": request,
        "active": "services",
        "services": service_list,
    })


@router.post("/services/{name}/restart", response_class=HTMLResponse)
async def restart_service(name: str, request: Request):
    """Terminate service process; watchdog will relaunch with back-off."""
    mgr: ServiceManager | None = getattr(request.app.state, "service_manager", None)
    if mgr is None:
        return HTMLResponse(
            '<span class="text-red-400 text-sm">ServiceManager not available.</span>'
        )

    matches = [s for s in mgr.list_services() if s.name == name]
    if not matches:
        return HTMLResponse(
            f'<span class="text-red-400 text-sm">Service <strong>{name}</strong> not found.</span>'
        )

    svc = matches[0]
    if svc._process and svc._process.returncode is None:
        svc._process.terminate()
        return HTMLResponse(
            f'<span class="text-[#FF9933] text-sm">Restarting <strong>{name}</strong>…</span>'
        )

    return HTMLResponse(
        f'<span class="text-stone-400 text-sm"><strong>{name}</strong> is not running.</span>'
    )
