"""Tests for app.routes.settings — connectors and WhatsApp QR endpoints."""

from __future__ import annotations

from unittest.mock import MagicMock

import pytest


def _fake_request(**state):
    """Return a minimal Request-like object with app.state attributes."""
    req = MagicMock()
    req.app.state.session_manager = None
    req.app.state.settings_store = None
    req.app.state.service_manager = None
    req.app.state.config = None
    req.app.state.platform_manager = None
    for k, v in state.items():
        setattr(req.app.state, k, v)
    return req


# ---------------------------------------------------------------------------
# Connectors API
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_list_connectors_empty():
    from app.routes.settings import list_connectors
    req = _fake_request()
    resp = await list_connectors(req)
    assert "connectors" in resp
    assert isinstance(resp["connectors"], list)
    # Should have discord, slack, telegram, whatsapp
    names = [c["name"] for c in resp["connectors"]]
    assert "discord" in names
    assert "slack" in names
    assert "telegram" in names
    assert "whatsapp" in names
