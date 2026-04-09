from __future__ import annotations

from app.routes.logs import _build_log_entries


def test_build_log_entries_pretty_prints_jsonl() -> None:
    entries = _build_log_entries(
        ['{"service":"agent","status":"ok"}\n', '{"count":2}\n'],
        "openagent-logs-2026-03-11.jsonl",
    )

    assert len(entries) == 2
    assert entries[0]["summary"].startswith("JSON record:")
    assert '"service": "openagent"' in entries[0]["pretty"]
    assert '"count": 2' in entries[1]["pretty"]


def test_build_log_entries_keeps_plain_text_logs() -> None:
    entries = _build_log_entries(
        ["first line\n", "second line\n"],
        "legacy.log",
    )

    assert entries == [
        {"summary": "Line 1", "pretty": "first line"},
        {"summary": "Line 2", "pretty": "second line"},
    ]


def test_build_log_entries_summarizes_otel_log_records() -> None:
    entries = _build_log_entries(
        ['{"resourceLogs":[{"scopeLogs":[{"logRecords":[{"severityText":"INFO","body":{"stringValue":"agent.step.ok"},"attributes":[{"key":"target","value":{"stringValue":"openagent::handlers"}}]}]}]}]}\n'],
        "openagent-logs-2026-03-11.jsonl",
    )

    assert entries[0]["summary"] == "INFO openagent::handlers: agent.step.ok"
