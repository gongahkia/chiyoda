"""Structured logging helper for chiyoda runtime.

Set ``CHIYODA_LOG_FORMAT=json`` for one-line JSON output. Default is human text.
"""

from __future__ import annotations

import json
import logging
import os
import sys
from typing import Any

_DEFAULT_NAME = "chiyoda"


class _JsonFormatter(logging.Formatter):
    def format(self, record: logging.LogRecord) -> str:
        payload: dict[str, Any] = {
            "ts": self.formatTime(record, datefmt="%Y-%m-%dT%H:%M:%S%z"),
            "level": record.levelname,
            "logger": record.name,
            "msg": record.getMessage(),
        }
        for key, value in record.__dict__.items():
            if key in {
                "args",
                "msg",
                "levelname",
                "levelno",
                "pathname",
                "filename",
                "module",
                "exc_info",
                "exc_text",
                "stack_info",
                "lineno",
                "funcName",
                "created",
                "msecs",
                "relativeCreated",
                "thread",
                "threadName",
                "processName",
                "process",
                "name",
                "message",
                "asctime",
                "taskName",
            }:
                continue
            payload[key] = value
        if record.exc_info:
            payload["exc"] = self.formatException(record.exc_info)
        return json.dumps(payload, default=str)


def get_logger(name: str = _DEFAULT_NAME) -> logging.Logger:
    logger = logging.getLogger(name)
    if getattr(logger, "_chiyoda_configured", False):
        return logger
    logger.setLevel(os.environ.get("CHIYODA_LOG_LEVEL", "INFO").upper())
    handler = logging.StreamHandler(sys.stderr)
    fmt = os.environ.get("CHIYODA_LOG_FORMAT", "text").lower()
    if fmt == "json":
        handler.setFormatter(_JsonFormatter())
    else:
        handler.setFormatter(
            logging.Formatter("%(asctime)s %(levelname)s %(name)s %(message)s")
        )
    logger.addHandler(handler)
    logger.propagate = False
    logger._chiyoda_configured = True
    return logger


def log_event(logger: logging.Logger | None, event: str, **fields: Any) -> None:
    """Emit a structured event. Falls back to the default chiyoda logger."""
    log = logger or get_logger()
    log.info(event, extra={"event": event, **fields})
