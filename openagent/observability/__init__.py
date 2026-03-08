"""OpenAgent observability package."""

from .context import ensure_request_id, get_request_id, set_request_id
from .logging import configure_logging, get_logger, log_event
from .metrics import render_metrics
from .otel import (
    baggage_get,
    baggage_set,
    current_span_id,
    current_trace_id,
    get_meter,
    get_tracer,
    setup_otel,
    shutdown_otel,
)

__all__ = [
    "ensure_request_id",
    "get_request_id",
    "set_request_id",
    "configure_logging",
    "get_logger",
    "log_event",
    "render_metrics",
    "setup_otel",
    "shutdown_otel",
    "get_tracer",
    "get_meter",
    "current_trace_id",
    "current_span_id",
    "baggage_set",
    "baggage_get",
]
