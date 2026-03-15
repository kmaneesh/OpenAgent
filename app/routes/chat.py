"""Chat route — GET /chat, WS /ws/chat, POST /api/chat/sessions/{id}/send"""

from __future__ import annotations

import json
import uuid

import httpx
from fastapi import APIRouter, WebSocket, WebSocketDisconnect
from fastapi.requests import Request
from fastapi.responses import StreamingResponse
from fastapi.templating import Jinja2Templates

router = APIRouter()
templates: Jinja2Templates  # injected by main.py


def _get_sessions(request: Request):
    """Return the SessionManager from app state."""
    app = request.app
    return getattr(app.state, "session_manager", None) or getattr(app.state, "sessions", None)


@router.get("/chat")
async def chat_page(request: Request):
    return templates.TemplateResponse("chat.html", {
        "request": request,
        "active": "chat",
    })


@router.get("/api/chat/sessions")
async def list_sessions(request: Request):
    """Return sessions with platform metadata for the operator sidebar."""
    sessions = _get_sessions(request)
    if not sessions:
        return {"sessions": []}

    keys = await sessions.list_sessions()
    result = []
    for key in keys:
        if key.startswith("user:"):
            links = await sessions.get_identity_links(key)
            if links:
                primary = links[0]
                result.append({
                    "key": key,
                    "platform": primary["platform"],
                    "channel_id": primary["channel_id"],
                    "platforms": links,
                })
                continue
        if ":" in key:
            platform, channel_id = key.split(":", 1)
        else:
            platform, channel_id = "unknown", key
        result.append({
            "key": key,
            "platform": platform,
            "channel_id": channel_id,
            "platforms": [{"platform": platform, "channel_id": channel_id, "last_active": None}],
        })

    return {"sessions": result}


@router.get("/api/chat/sessions/{session_id}/history")
async def get_history(request: Request, session_id: str):
    sessions = _get_sessions(request)
    if not sessions:
        return {"history": []}
    history = await sessions.get_history(session_id)
    out = [
        {"role": t.role, "content": t.content, "timestamp": t.timestamp.isoformat()}
        for t in history
    ]
    return {"history": out}


@router.delete("/api/chat/sessions/{session_id}")
async def delete_session(request: Request, session_id: str):
    """Soft-delete: hide session from sidebar while keeping turns for logs."""
    sessions = _get_sessions(request)
    if not sessions:
        return {"error": "session manager unavailable"}
    await sessions.hide_session(session_id)
    return {"ok": True}


@router.post("/api/chat/sessions/{session_id}/send")
async def direct_send(request: Request, session_id: str):
    """Operator direct reply — calls Rust POST /tool/channel.send."""
    body = await request.json()
    content = str(body.get("content", "")).strip()
    if not content:
        return {"error": "content required"}

    if session_id.startswith("user:"):
        sessions = _get_sessions(request)
        if sessions:
            links = await sessions.get_identity_links(session_id)
            if links:
                primary = links[0]
                platform = primary["platform"]
                channel_id = primary["channel_id"]
            else:
                return {"error": "no platform links found for this session"}
        else:
            return {"error": "session manager unavailable"}
    elif ":" in session_id:
        platform, channel_id = session_id.split(":", 1)
    else:
        return {"error": "cannot determine platform from session id"}

    if platform == "web":
        return {"error": "use WebSocket for web sessions"}

    api_client: httpx.AsyncClient = request.app.state.api_client
    channel_uri = f"{platform}://{channel_id}"
    try:
        resp = await api_client.post(f"/tool/channel.send", content=json.dumps({
            "address": channel_uri,
            "content": content,
        }), headers={"Content-Type": "application/json"})
        return {"ok": resp.is_success, "platform": platform, "channel_id": channel_id}
    except Exception as e:
        return {"error": str(e)}


@router.websocket("/ws/chat")
async def chat_ws(ws: WebSocket):
    await ws.accept()

    app = ws.app
    api_client: httpx.AsyncClient = app.state.api_client

    # Prefer requested session ID, otherwise generate unique tab ID.
    requested_session = ws.query_params.get("session_id")
    if requested_session:
        session_id = requested_session
    else:
        session_id = f"web:{uuid.uuid4().hex[:12]}"

    # Tell the browser its session ID so it can display it.
    await ws.send_json({"session_id": session_id})

    try:
        while True:
            text = await ws.receive_text()
            if not text.strip():
                continue

            try:
                resp = await api_client.post("/step", json={
                    "platform": "web",
                    "channel_id": session_id,
                    "session_id": session_id,
                    "user_input": text.strip(),
                })
                resp.raise_for_status()
                data = resp.json()
                response_text = data.get("response_text", "")
                await ws.send_json({"role": "agent", "content": response_text})
            except httpx.HTTPStatusError as e:
                await ws.send_json({"role": "error", "content": f"Agent error: {e.response.status_code}"})
            except Exception as e:
                await ws.send_json({"role": "error", "content": str(e)})
    except WebSocketDisconnect:
        pass
