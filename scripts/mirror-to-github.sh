#!/bin/sh
# Mirrors this repository to GitHub WITHOUT the .github/ folder (builds run on Forgejo).
#
# GitHub's main is this history minus .github/. New commits are appended on top of it
# (never force-pushed): each commit not on GitHub yet is recreated with the .github/
# entry removed from its tree. Commits that only changed .github/ are skipped. Author
# and committer name/email are taken from GitHub's current tip, so the identity you
# publish under stays the same; dates and messages are kept.
#
# Usage: scripts/mirror-to-github.sh [TAG]
#   TAG       also push this release tag, pointing at the mirrored commit (never moves an existing tag)
# Env:
#   GH_REPO   owner/name on GitHub (default mabayhan/not-magick-mouse)
#   GH_TOKEN  token with push access (CI). Without it the local git remote "github" is used.
#   DRY_RUN=1 show what would be pushed, push nothing
# Needs a full clone (fetch-depth 0 in CI).
set -eu

repo=${GH_REPO:-mabayhan/not-magick-mouse}
if [ -n "${GH_TOKEN:-}" ]; then
    remote="https://github.com/$repo.git"
    auth=$(printf 'x-access-token:%s' "$GH_TOKEN" | base64 | tr -d '\n')
    g() { git -c "http.extraheader=Authorization: Basic $auth" "$@"; }
else
    remote=github
    g() { git "$@"; }
fi
tag=${1:-}

g fetch -q "$remote" "+refs/heads/main:refs/mirror/main"
remote_head=$(git rev-parse refs/mirror/main)
remote_tree=$(git rev-parse "$remote_head^{tree}")

# The tree of a commit without the .github entry.
ftree() { git ls-tree "$1" | awk -F'\t' '$2 != ".github"' | git mktree; }

# Newest commit whose filtered tree is what GitHub has: where GitHub is in our history.
base=""
for c in $(git rev-list HEAD); do
    if [ "$(ftree "$c")" = "$remote_tree" ]; then base=$c; break; fi
done
[ -n "$base" ] || { echo "error: GitHub main does not correspond to any commit in this history" >&2; exit 1; }

an=$(git log -1 --format=%an "$remote_head"); ae=$(git log -1 --format=%ae "$remote_head")
map=$(mktemp)
trap 'rm -f "$map"' EXIT
parent=$remote_head
parent_tree=$remote_tree
n=0
for c in $(git rev-list --reverse "$base..HEAD"); do
    t=$(ftree "$c")
    if [ "$t" != "$parent_tree" ]; then
        parent=$(git log -1 --format=%B "$c" | \
            GIT_AUTHOR_NAME=$an GIT_AUTHOR_EMAIL=$ae GIT_AUTHOR_DATE=$(git log -1 --format=%aI "$c") \
            GIT_COMMITTER_NAME=$an GIT_COMMITTER_EMAIL=$ae GIT_COMMITTER_DATE=$(git log -1 --format=%cI "$c") \
            git commit-tree "$t" -p "$parent")
        parent_tree=$t
        n=$((n + 1))
        echo "mirror $(git log -1 --format='%h %s' "$c")  ->  $(echo "$parent" | cut -c1-7)"
    fi
    echo "$c $parent" >> "$map"
done

if [ "$parent" = "$remote_head" ]; then
    echo "GitHub main is already up to date ($(echo "$remote_head" | cut -c1-7))."
else
    echo "Pushing $n commit(s) to GitHub main."
    [ -n "${DRY_RUN:-}" ] || g push "$remote" "$parent:refs/heads/main"
fi

if [ -n "$tag" ]; then
    tc=$(git rev-parse "$tag^{commit}")
    if [ "$tc" = "$base" ]; then
        target=$remote_head
    else
        target=$(awk -v c="$tc" '$1 == c { print $2 }' "$map")
    fi
    [ -n "$target" ] || { echo "error: tag $tag is older than GitHub main; refusing" >&2; exit 1; }
    if g ls-remote --exit-code --tags "$remote" "refs/tags/$tag" >/dev/null 2>&1; then
        echo "Tag $tag already exists on GitHub; leaving it."
    else
        echo "Tagging $tag -> $(echo "$target" | cut -c1-7) on GitHub."
        [ -n "${DRY_RUN:-}" ] || g push "$remote" "$target:refs/tags/$tag"
    fi
fi
