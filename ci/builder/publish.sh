#!/bin/sh
# Builds the CI builder image and pushes it to the Forgejo container registry.
# Runs on a machine with podman or docker (Forgejo runner jobs have no Docker daemon).
# Usage: FORGEJO_TOKEN=<token with write:package> ci/builder/publish.sh
# Env:   FORGEJO_URL (default https://forge.bayhan.ca), FORGEJO_OWNER (default mali)
# Rebuild when ci/builder/Dockerfile changes, and now and then to pick up a new Rust release.
set -eu
cd "$(dirname "$0")"
url=${FORGEJO_URL:-https://forge.bayhan.ca}
host=${url#*://}
owner=${FORGEJO_OWNER:-mali}
: "${FORGEJO_TOKEN:?set FORGEJO_TOKEN (scope write:package)}"
engine=$(command -v podman || command -v docker || { echo "install podman or docker" >&2; exit 1; })
image="$host/$owner/nmm-builder:latest"
"$engine" build -t "$image" .
echo "$FORGEJO_TOKEN" | "$engine" login "$host" -u "$owner" --password-stdin
"$engine" push "$image"
echo "Pushed $image"
