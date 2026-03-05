"""Settings route — GET /settings (Provider + Identity tabs)."""

from __future__ import annotations

from fastapi import APIRouter, Request
from fastapi.templating import Jinja2Templates
from pydantic import BaseModel

router = APIRouter()
templates: Jinja2Templates  # injected by main.py


def _sessions(request: Request):
    app = request.app
    return getattr(app.state, "session_manager", None) or getattr(app.state, "sessions", None)


# ---------------------------------------------------------------------------
# Page
# ---------------------------------------------------------------------------

@router.get("/settings")
async def settings_page(request: Request):
    return templates.TemplateResponse("settings.html", {
        "request": request,
        "active": "settings",
    })


# ---------------------------------------------------------------------------
# Users API
# ---------------------------------------------------------------------------

class UserPatch(BaseModel):
    name: str = ""
    email: str = ""


@router.get("/api/settings/users")
async def list_users(request: Request):
    sessions = _sessions(request)
    if not sessions:
        return {"users": []}
    users = await sessions.list_users()
    # Attach identity links per user
    result = []
    for u in users:
        links = await sessions.get_identity_links(u["user_key"])
        result.append({**u, "platforms": links})
    return {"users": result}


@router.post("/api/settings/users")
async def create_user(request: Request, body: UserPatch):
    """Create a bare user record (no platform identity yet)."""
    import uuid
    sessions = _sessions(request)
    if not sessions:
        return {"error": "session manager unavailable"}
    user_key = f"user:{uuid.uuid4().hex[:16]}"
    await sessions.upsert_user(user_key, name=body.name, email=body.email)
    return {"ok": True, "user_key": user_key}


@router.patch("/api/settings/users/{user_key}")
async def update_user(request: Request, user_key: str, body: UserPatch):
    sessions = _sessions(request)
    if not sessions:
        return {"error": "session manager unavailable"}
    await sessions.upsert_user(user_key, name=body.name, email=body.email)
    return {"ok": True}


@router.delete("/api/settings/users/{user_key}")
async def delete_user(request: Request, user_key: str):
    sessions = _sessions(request)
    if not sessions:
        return {"error": "session manager unavailable"}
    await sessions.delete_user(user_key)
    return {"ok": True}


# ---------------------------------------------------------------------------
# Identity links API
# ---------------------------------------------------------------------------

PLATFORMS = {"web", "discord", "telegram", "slack", "whatsapp"}


class LinkBody(BaseModel):
    user_key: str
    platform: str
    platform_id: str
    channel_id: str = ""


@router.get("/api/settings/identity")
async def list_identities(request: Request):
    """Return all identity links grouped by user_key."""
    sessions = _sessions(request)
    if not sessions:
        return {"identities": []}
    rows = await sessions.list_all_identities()
    # Group by user_key
    grouped: dict[str, list] = {}
    for r in rows:
        grouped.setdefault(r["user_key"], []).append({
            "platform": r["platform"],
            "platform_id": r["platform_id"],
            "channel_id": r["channel_id"],
            "last_active": r["last_active"],
        })
    return {
        "identities": [
            {"user_key": k, "platforms": v} for k, v in grouped.items()
        ]
    }


@router.post("/api/settings/identity/link")
async def add_identity_link(request: Request, body: LinkBody):
    """Manually link a platform identity to a user_key."""
    sessions = _sessions(request)
    if not sessions:
        return {"error": "session manager unavailable"}
    if body.platform not in PLATFORMS:
        return {"error": f"unknown platform '{body.platform}'. Valid: {sorted(PLATFORMS)}"}
    if not body.platform_id.strip():
        return {"error": "platform_id required"}
    await sessions.set_identity_link(
        body.user_key, body.platform, body.platform_id.strip(), body.channel_id.strip()
    )
    return {"ok": True}


@router.delete("/api/settings/identity/{platform}/{platform_id:path}")
async def remove_identity_link(request: Request, platform: str, platform_id: str):
    """Remove a specific platform identity link."""
    sessions = _sessions(request)
    if not sessions:
        return {"error": "session manager unavailable"}
    await sessions.unlink_platform(platform, platform_id)
    return {"ok": True}


@router.post("/api/settings/identity/merge")
async def merge_sessions(request: Request):
    """Merge key_b into key_a — all turns and identities move to key_a."""
    body = await request.json()
    key_a = str(body.get("key_a", "")).strip()
    key_b = str(body.get("key_b", "")).strip()
    if not key_a or not key_b:
        return {"error": "key_a and key_b required"}
    if key_a == key_b:
        return {"error": "cannot merge a session with itself"}
    sessions = _sessions(request)
    if not sessions:
        return {"error": "session manager unavailable"}
    winner = await sessions.link_user_keys(key_a, key_b)
    return {"ok": True, "winner": winner}
