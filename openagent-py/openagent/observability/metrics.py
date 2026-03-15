from __future__ import annotations

from prometheus_client import CONTENT_TYPE_LATEST, Counter, Histogram, generate_latest

MCP_REQUEST_TOTAL = Counter(
    "openagent_mcplite_request_total",
    "MCP-lite request outcomes by service/tool/status.",
    labelnames=("service", "type", "tool", "status"),
)

MCP_REQUEST_SECONDS = Histogram(
    "openagent_mcplite_request_seconds",
    "MCP-lite request latency in seconds.",
    labelnames=("service", "type", "tool", "status"),
    buckets=(0.001, 0.003, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1, 2, 5),
)

MCP_EVENTS_TOTAL = Counter(
    "openagent_mcplite_events_total",
    "MCP-lite events observed from services.",
    labelnames=("service", "event"),
)

PROVIDER_CALL_TOTAL = Counter(
    "openagent_provider_call_total",
    "Provider call outcomes by provider/operation/status.",
    labelnames=("provider", "operation", "status", "error_type"),
)

PROVIDER_CALL_SECONDS = Histogram(
    "openagent_provider_call_seconds",
    "Provider call latency in seconds.",
    labelnames=("provider", "operation", "status"),
    buckets=(0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1, 2, 5, 10, 30),
)


def render_metrics() -> tuple[bytes, str]:
    return generate_latest(), CONTENT_TYPE_LATEST
