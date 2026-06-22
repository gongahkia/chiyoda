FROM python:3.12-slim AS builder

WORKDIR /build
COPY requirements-lock.txt ./
RUN python -m venv /opt/venv \
    && /opt/venv/bin/pip install --no-cache-dir --upgrade pip \
    && /opt/venv/bin/pip install --no-cache-dir -r requirements-lock.txt

FROM python:3.12-slim AS runtime

ENV PATH="/opt/venv/bin:$PATH" \
    PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1

WORKDIR /app
COPY --from=builder /opt/venv /opt/venv
COPY chiyoda chiyoda
COPY scenarios scenarios
COPY docs/benchmark docs/benchmark
COPY data data

ENTRYPOINT ["python", "-m", "chiyoda.cli"]
CMD ["--help"]
