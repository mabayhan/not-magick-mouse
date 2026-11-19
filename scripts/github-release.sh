#!/bin/sh
# Creates (or reuses) a GitHub release for TAG and uploads files to it.
# Usage: GH_TOKEN=... scripts/github-release.sh TAG FILE...
# Env:   GH_REPO owner/name (default mabayhan/not-magick-mouse), DRY_RUN=1 to only print.
# The tag must already be on GitHub (scripts/mirror-to-github.sh TAG does that).
set -eu

repo=${GH_REPO:-mabayhan/not-magick-mouse}
tag=${1:?usage: github-release.sh TAG FILE...}
shift
api=https://api.github.com/repos/$repo
resp=$(mktemp)
trap 'rm -f "$resp"' EXIT

if [ -n "${DRY_RUN:-}" ]; then
    echo "would create/reuse release $tag on $repo and upload: $*"
    exit 0
fi
: "${GH_TOKEN:?set GH_TOKEN}"

call() { # method url [curl args...]
    m=$1; u=$2; shift 2
    curl -s -o "$resp" -w '%{http_code}' -X "$m" -H "Authorization: Bearer $GH_TOKEN" \
        -H "Accept: application/vnd.github+json" "$@" "$u"
}
jsonid() { python3 -c 'import sys, json; print(json.load(sys.stdin)["id"])' < "$resp"; }

code=$(call GET "$api/releases/tags/$tag")
if [ "$code" = 200 ]; then
    echo "Release $tag exists; adding files."
else
    code=$(call POST "$api/releases" -d "{\"tag_name\":\"$tag\",\"name\":\"$tag\",\"generate_release_notes\":true}")
    [ "$code" = 201 ] || { echo "error: creating release failed (HTTP $code)" >&2; cat "$resp" >&2; exit 1; }
    echo "Created release $tag."
fi
id=$(jsonid)

for f in "$@"; do
    name=$(basename "$f")
    code=$(call POST "https://uploads.github.com/repos/$repo/releases/$id/assets?name=$name" \
        -H "Content-Type: application/octet-stream" --data-binary "@$f")
    case "$code" in
        201) echo "uploaded $name" ;;
        422) echo "skipped $name (already uploaded)" ;;
        *) echo "error: uploading $name failed (HTTP $code)" >&2; cat "$resp" >&2; exit 1 ;;
    esac
done
