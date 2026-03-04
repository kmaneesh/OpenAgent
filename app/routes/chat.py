"""Chat route — GET /chat, WS /ws/chat"""

from __future__ import annotations

import asyncio

from fastapi import APIRouter, Request, WebSocket, WebSocketDisconnect
from fastapi.templating import Jinja2Templates

router = APIRouter()
templates: Jinja2Templates  # injected by main.py


@router.get("/chat")
async def chat_page(request: Request):
    return templates.TemplateResponse("chat.html", {
        "request": request,
        "active": "chat",
    })


@router.websocket("/ws/chat")
async def chat_ws(ws: WebSocket):
    await ws.accept()
    try:
        while True:
            text = await ws.receive_text()
            if not text.strip():
                continue
            # Stub: agent loop not built yet
            await asyncio.sleep(0.3)
            await ws.send_json({
                "role": "agent",
                "content": (
                    "Agent loop not yet connected. "
                    "Build the core agent loop extension to enable real responses."
                ),
            })
    except WebSocketDisconnect:
        pass
