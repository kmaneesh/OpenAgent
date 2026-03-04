from __future__ import annotations

from types import SimpleNamespace

from app.routes import extensions


def test_get_extensions_uses_entrypoint_name_for_package(monkeypatch):
    ep1 = SimpleNamespace(name="discord", value="discord_plugin:DiscordExtension")
    ep2 = SimpleNamespace(name="whatsapp", value="plugin:WhatsAppExtension")

    monkeypatch.setattr(
        extensions.importlib.metadata,
        "entry_points",
        lambda group: [ep1, ep2] if group == "openagent.extensions" else [],
    )

    def _fake_distribution(_pkg: str):
        raise RuntimeError("not installed in this test")

    monkeypatch.setattr(extensions.importlib.metadata, "distribution", _fake_distribution)

    rows = extensions._get_extensions()

    assert rows[0]["name"] == "discord"
    assert rows[0]["package"] == "discord"
    assert rows[0]["entry_point"] == "discord_plugin:DiscordExtension"

    assert rows[1]["name"] == "whatsapp"
    assert rows[1]["package"] == "whatsapp"
    assert rows[1]["entry_point"] == "plugin:WhatsAppExtension"
