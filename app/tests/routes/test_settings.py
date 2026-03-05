"""Tests for app.routes.settings — users CRUD and identity link endpoints."""

from __future__ import annotations

from pathlib import Path
from unittest.mock import AsyncMock, MagicMock

import pytest
import pytest_asyncio

from openagent.session import SessionManager, SqliteSessionBackend


# ---------------------------------------------------------------------------
# Fixtures — real in-memory session manager
# ---------------------------------------------------------------------------


@pytest_asyncio.fixture
async def sessions(tmp_path: Path) -> SessionManager:
    backend = SqliteSessionBackend(tmp_path / "test.db")
    mgr = SessionManager(backend=backend)
    await mgr.start()
    yield mgr
    await mgr.stop()


def _fake_request(sessions_mgr):
    """Return a minimal Request-like object wired to a SessionManager."""
    req = MagicMock()
    req.app.state.session_manager = sessions_mgr
    return req


# ---------------------------------------------------------------------------
# Users CRUD
# ---------------------------------------------------------------------------


class TestUsersAPI:
    @pytest.mark.asyncio
    async def test_list_users_empty(self, sessions):
        from app.routes.settings import list_users
        req = _fake_request(sessions)
        resp = await list_users(req)
        assert resp == {"users": []}

    @pytest.mark.asyncio
    async def test_create_and_list_user(self, sessions):
        from app.routes.settings import create_user, list_users, UserPatch
        req = _fake_request(sessions)

        resp = await create_user(req, UserPatch(name="Alice", email="alice@example.com"))
        assert resp["ok"] is True
        user_key = resp["user_key"]
        assert user_key.startswith("user:")

        list_resp = await list_users(req)
        assert len(list_resp["users"]) == 1
        u = list_resp["users"][0]
        assert u["user_key"] == user_key
        assert u["name"] == "Alice"

    @pytest.mark.asyncio
    async def test_update_user(self, sessions):
        from app.routes.settings import create_user, update_user, list_users, UserPatch
        req = _fake_request(sessions)

        create_resp = await create_user(req, UserPatch(name="Bob", email="bob@example.com"))
        user_key = create_resp["user_key"]

        patch_resp = await update_user(req, user_key, UserPatch(name="Bobby", email="bobby@example.com"))
        assert patch_resp["ok"] is True

        list_resp = await list_users(req)
        u = list_resp["users"][0]
        assert u["name"] == "Bobby"
        assert u["email"] == "bobby@example.com"

    @pytest.mark.asyncio
    async def test_delete_user(self, sessions):
        from app.routes.settings import create_user, delete_user, list_users, UserPatch
        req = _fake_request(sessions)

        create_resp = await create_user(req, UserPatch(name="Carol"))
        user_key = create_resp["user_key"]

        del_resp = await delete_user(req, user_key)
        assert del_resp["ok"] is True

        list_resp = await list_users(req)
        assert list_resp["users"] == []

    @pytest.mark.asyncio
    async def test_create_user_no_session_manager(self):
        from app.routes.settings import create_user, UserPatch
        req = MagicMock()
        req.app.state.session_manager = None
        req.app.state.sessions = None

        resp = await create_user(req, UserPatch(name="X"))
        assert "error" in resp

    @pytest.mark.asyncio
    async def test_list_users_no_session_manager(self):
        from app.routes.settings import list_users
        req = MagicMock()
        req.app.state.session_manager = None
        req.app.state.sessions = None

        resp = await list_users(req)
        assert resp == {"users": []}


# ---------------------------------------------------------------------------
# Identity links
# ---------------------------------------------------------------------------


class TestIdentityAPI:
    @pytest.mark.asyncio
    async def test_list_identities_empty(self, sessions):
        from app.routes.settings import list_identities
        req = _fake_request(sessions)
        resp = await list_identities(req)
        assert resp == {"identities": []}

    @pytest.mark.asyncio
    async def test_add_and_list_identity(self, sessions):
        from app.routes.settings import add_identity_link, list_identities, LinkBody, create_user, UserPatch
        req = _fake_request(sessions)

        # Create user first
        cr = await create_user(req, UserPatch(name="Dave"))
        user_key = cr["user_key"]

        link_resp = await add_identity_link(req, LinkBody(
            user_key=user_key,
            platform="telegram",
            platform_id="12345",
            channel_id="12345",
        ))
        assert link_resp["ok"] is True

        list_resp = await list_identities(req)
        assert len(list_resp["identities"]) == 1
        entry = list_resp["identities"][0]
        assert entry["user_key"] == user_key
        assert any(p["platform"] == "telegram" for p in entry["platforms"])

    @pytest.mark.asyncio
    async def test_remove_identity_link(self, sessions):
        from app.routes.settings import add_identity_link, remove_identity_link, list_identities, LinkBody, create_user, UserPatch
        req = _fake_request(sessions)

        cr = await create_user(req, UserPatch(name="Eve"))
        user_key = cr["user_key"]

        await add_identity_link(req, LinkBody(
            user_key=user_key, platform="slack", platform_id="UABC", channel_id="C1",
        ))

        del_resp = await remove_identity_link(req, "slack", "UABC")
        assert del_resp["ok"] is True

        list_resp = await list_identities(req)
        assert list_resp["identities"] == []

    @pytest.mark.asyncio
    async def test_add_identity_unknown_platform(self, sessions):
        from app.routes.settings import add_identity_link, LinkBody
        req = _fake_request(sessions)
        resp = await add_identity_link(req, LinkBody(
            user_key="user:abc", platform="fakeplatform", platform_id="1",
        ))
        assert "error" in resp

    @pytest.mark.asyncio
    async def test_add_identity_blank_platform_id(self, sessions):
        from app.routes.settings import add_identity_link, LinkBody
        req = _fake_request(sessions)
        resp = await add_identity_link(req, LinkBody(
            user_key="user:abc", platform="discord", platform_id="   ",
        ))
        assert "error" in resp

    @pytest.mark.asyncio
    async def test_add_identity_no_session_manager(self):
        from app.routes.settings import add_identity_link, LinkBody
        req = MagicMock()
        req.app.state.session_manager = None
        req.app.state.sessions = None
        resp = await add_identity_link(req, LinkBody(
            user_key="user:x", platform="discord", platform_id="99",
        ))
        assert "error" in resp


# ---------------------------------------------------------------------------
# Merge sessions
# ---------------------------------------------------------------------------


class TestMergeAPI:
    @pytest.mark.asyncio
    async def test_merge_sessions(self, sessions):
        from app.routes.settings import merge_sessions, add_identity_link, list_identities, LinkBody, create_user, UserPatch
        req = _fake_request(sessions)

        # Create two users and give them a platform identity each
        ca = await create_user(req, UserPatch(name="UserA"))
        cb = await create_user(req, UserPatch(name="UserB"))
        key_a, key_b = ca["user_key"], cb["user_key"]

        await add_identity_link(req, LinkBody(user_key=key_a, platform="discord", platform_id="d1"))
        await add_identity_link(req, LinkBody(user_key=key_b, platform="telegram", platform_id="t1"))

        req.json = AsyncMock(return_value={"key_a": key_a, "key_b": key_b})
        resp = await merge_sessions(req)
        assert resp["ok"] is True
        assert "winner" in resp

    @pytest.mark.asyncio
    async def test_merge_same_key_rejected(self, sessions):
        from app.routes.settings import merge_sessions
        req = _fake_request(sessions)
        req.json = AsyncMock(return_value={"key_a": "user:abc", "key_b": "user:abc"})
        resp = await merge_sessions(req)
        assert "error" in resp

    @pytest.mark.asyncio
    async def test_merge_missing_keys_rejected(self, sessions):
        from app.routes.settings import merge_sessions
        req = _fake_request(sessions)
        req.json = AsyncMock(return_value={"key_a": "", "key_b": ""})
        resp = await merge_sessions(req)
        assert "error" in resp

    @pytest.mark.asyncio
    async def test_merge_no_session_manager(self):
        from app.routes.settings import merge_sessions
        req = MagicMock()
        req.app.state.session_manager = None
        req.app.state.sessions = None
        req.json = AsyncMock(return_value={"key_a": "user:a", "key_b": "user:b"})
        resp = await merge_sessions(req)
        assert "error" in resp
