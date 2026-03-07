"""Extensions route — GET /extensions"""

from __future__ import annotations

import tomllib
from pathlib import Path
from typing import Any

from fastapi import APIRouter, Request
from fastapi.templating import Jinja2Templates

router = APIRouter()
templates: Jinja2Templates  # injected by main.py


def _extensions_from_directory(root: Path) -> list[dict[str, Any]]:
    """Scan root/extensions/ for Python extension packages (those with pyproject.toml)."""
    extensions_dir = root / "extensions"
    result: list[dict[str, Any]] = []

    if not extensions_dir.exists():
        return result

    for ext_dir in sorted(extensions_dir.iterdir()):
        if not ext_dir.is_dir():
            continue
        pyproject = ext_dir / "pyproject.toml"
        if not pyproject.exists():
            continue
        try:
            data = tomllib.loads(pyproject.read_text(encoding="utf-8"))
            project = data.get("project", {})
            pkg_name = project.get("name", ext_dir.name)
            version = str(project.get("version", "?"))
            entry_points = project.get("entry-points", {}) or project.get("entry_points", {}) or {}
            oa_eps = entry_points.get("openagent.extensions", {})
            entry_point = next(
                (f"{k} = {v}" for k, v in oa_eps.items()),
                "",
            )
            result.append({
                "name": ext_dir.name,
                "package": pkg_name,
                "distribution": pkg_name,
                "version": version,
                "entry_point": entry_point,
                "status": "registered",
            })
        except (tomllib.TOMLDecodeError, OSError):
            result.append({
                "name": ext_dir.name,
                "package": ext_dir.name,
                "distribution": ext_dir.name,
                "version": "?",
                "entry_point": "",
                "status": "error",
            })

    return result


@router.get("/extensions")
async def extensions_page(request: Request):
    root = getattr(request.app.state, "root", Path.cwd())
    return templates.TemplateResponse("extensions.html", {
        "request": request,
        "active": "extensions",
        "extensions": _extensions_from_directory(root),
    })
