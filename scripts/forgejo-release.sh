#!/bin/sh
# Creates (or reuses) a Forgejo release for TAG and uploads files to it, using the Forgejo API.
# Usage: FORGEJO_TOKEN=... scripts/forgejo-release.sh TAG FILE...
# Env:   FORGEJO_URL   instance URL (default https://forge.bayhan.ca)
#        FORGEJO_REPO  owner/name (default mali/not-magick-mouse)
#        DRY_RUN=1     only print
# Release notes are the commit subjects since the previous tag (run in a full clone).
set -eu

url=${FORGEJO_URL:-https://forge.bayhan.ca}
repo=${FORGEJO_REPO:-mali/not-magick-mouse}
tag=${1:?usage: forgejo-release.sh TAG FILE...}
shift
api="${url%/}/api/v1/repos/$repo"

if [ -n "${DRY_RUN:-}" ]; then
    echo "would create/reuse release $tag on $api and upload: $*"
    exit 0
fi
: "${FORGEJO_TOKEN:?set FORGEJO_TOKEN}"

resp=$(mktemp)
trap 'rm -f "$resp"' EXIT

call() { # method url [curl args...]
    m=$1; u=$2; shift 2
    curl -s -o "$resp" -w '%{http_code}' -X "$m" -H "Authorization: token $FORGEJO_TOKEN" \
        -H "Accept: application/json" "$@" "$u"
}
jsonid() { python3 -c 'import sys, json; print(json.load(sys.stdin)["id"])' < "$resp"; }

code=$(call GET "$api/releases/tags/$tag")
if [ "$code" = 200 ]; then
    echo "Release $tag exists; adding files."
else
    prev=$(git describe --tags --abbrev=0 "$tag^" 2>/dev/null || true)
    if [ -n "$prev" ]; then range="$prev..$tag"; else range="$tag"; fi
    body=$(git log --format='- %s' "$range" 2>/dev/null | head -n 50 || true)
    payload=$(TAG=$tag BODY=$body python3 -c 'import os, json; print(json.dumps({"tag_name": os.environ["TAG"], "name": os.environ["TAG"], "body": os.environ["BODY"], "draft": False, "prerelease": False}))')
    code=$(call POST "$api/releases" -H "Content-Type: application/json" -d "$payload")
    [ "$code" = 201 ] || { echo "error: creating release failed (HTTP $code)" >&2; cat "$resp" >&2; exit 1; }
    echo "Created release $tag."
fi
id=$(jsonid)

for f in "$@"; do
    name=$(basename "$f")
    code=$(call POST "$api/releases/$id/assets?name=$name" -F "attachment=@$f")
    case "$code" in
        201) echo "uploaded $name" ;;
        409|422) echo "skipped $name (already uploaded)" ;;
        *) echo "error: uploading $name failed (HTTP $code)" >&2; cat "$resp" >&2; exit 1 ;;
    esac
done
