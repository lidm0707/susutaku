#!/bin/sh
# Relay the OAuth loopback callback into container loopback.
#
# `codex login` binds its callback server to 127.0.0.1:1455 inside the
# container, but Docker's published port arrives on the container's eth0
# interface — so the browser's redirect would die with an empty response.
# socat bridges eth0:1455 -> 127.0.0.1:1455 (no conflict: loopback and the
# interface IP are distinct bind addresses).
set -e

CONTAINER_IP=$(hostname -i | awk '{print $1}')
if [ -n "$CONTAINER_IP" ]; then
    socat "TCP4-LISTEN:1455,bind=${CONTAINER_IP},fork,reuseaddr" \
        "TCP4:127.0.0.1:1455" &
fi

# Build the default sandbox image once (cached in the podman storage volume
# across restarts; skipped when it already exists).
if command -v podman >/dev/null 2>&1 \
    && ! podman image exists localhost/susutaku-sandbox:latest; then
    podman build -f /usr/share/susutaku/sandbox/Containerfile \
        -t localhost/susutaku-sandbox:latest /usr/share/susutaku/sandbox \
        || echo "sandbox image build failed; sandbox runs will error until it exists"
fi

exec /usr/local/bin/backend
