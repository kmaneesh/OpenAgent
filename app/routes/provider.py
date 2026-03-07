"""Provider config API — GET /api/config/provider + PATCH /api/config/provider"""

from __future__ import annotations

from typing import Literal

import httpx
from fastapi import APIRouter, Request
from pydantic import BaseModel

from openagent.providers import ProviderConfig, get_provider

router = APIRouter(prefix="/api/config")


class ProviderPatch(BaseModel):
    kind: Literal["openai_compat", "anthropic", "openai"] | None = None
    base_url: str | None = None
    api_key: str | None = None
    model: str | None = None
    timeout: float | None = None
    max_tokens: int | None = None


@router.get("/provider")
async def get_provider_config(request: Request):
    cfg: ProviderConfig = request.app.state.provider_config
    data = cfg.model_dump()
    data.pop("api_key", None)  # never expose key over the API
    return data


def _settings_store(request: Request):
    return getattr(request.app.state, "settings_store", None)


@router.patch("/provider")
async def patch_provider_config(request: Request, body: ProviderPatch):
    cfg: ProviderConfig = request.app.state.provider_config
    updates = {k: v for k, v in body.model_dump().items() if v is not None}
    new_cfg = cfg.model_copy(update=updates)
    request.app.state.provider_config = new_cfg
    request.app.state.active_provider = get_provider(new_cfg)
    store = _settings_store(request)
    if store:
        for k, v in updates.items():
            if k != "api_key" or v:
                await store.set(f"provider.{k}", str(v))
    data = new_cfg.model_dump()
    data.pop("api_key", None)
    return {"ok": True, "config": data}


@router.get("/models")
async def get_provider_models(request: Request, base_url: str | None = None):
    """Fetch model IDs from the provider's /models endpoint (OpenAI-compatible)."""
    cfg: ProviderConfig = request.app.state.provider_config
    url = (base_url or cfg.base_url or "").rstrip("/")
    if not url or cfg.kind == "anthropic":
        return {"ok": False, "models": [], "error": "No base URL or Anthropic does not expose /models"}
    try:
        async with httpx.AsyncClient(timeout=6.0) as client:
            r = await client.get(f"{url}/models")
            r.raise_for_status()
            data = r.json()
            models = [m["id"] for m in data.get("data", []) if "id" in m]
        return {"ok": True, "models": sorted(models), "error": None}
    except Exception as exc:
        return {"ok": False, "models": [], "error": str(exc)}
