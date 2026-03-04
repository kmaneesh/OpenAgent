"""Extensions route — GET /extensions"""

from __future__ import annotations

import importlib.metadata

from fastapi import APIRouter, Request
from fastapi.templating import Jinja2Templates

router = APIRouter()
templates: Jinja2Templates  # injected by main.py


def _get_extensions() -> list[dict]:
    eps = importlib.metadata.entry_points(group="openagent.extensions")
    result = []
    for ep in eps:
        pkg_name = ep.value.split(":")[0].split(".")[0]
        try:
            dist = importlib.metadata.distribution(pkg_name)
            version = dist.metadata["Version"]
            dist_name = dist.metadata["Name"]
        except Exception:
            version = "?"
            dist_name = pkg_name
        # UI should show canonical extension id (entry-point name), not module filenames.
        display_name = ep.name
        result.append({
            "name": ep.name,
            "package": display_name,
            "distribution": dist_name,
            "version": version,
            "entry_point": ep.value,
            "status": "registered",
        })
    return result


@router.get("/extensions")
async def extensions_page(request: Request):
    return templates.TemplateResponse("extensions.html", {
        "request": request,
        "active": "extensions",
        "extensions": _get_extensions(),
    })
