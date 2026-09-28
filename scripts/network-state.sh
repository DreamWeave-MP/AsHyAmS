#!/usr/bin/env bash
# The network-state branch, as a worktree at state/.
#
#   scripts/network-state.sh load            check out the branch at state/, or start it if it
#                                            does not exist yet
#   scripts/network-state.sh save SUMMARY    commit state/ with SUMMARY as the message body and
#                                            push it; does nothing when nothing changed
#
# The same commands work in CI and on a maintainer's clone. The remote is `origin`.
set -euo pipefail

branch=network-state
directory=state
root=$(git rev-parse --show-toplevel)
cd "$root"

load() {
    if [ -e "$directory/.git" ]; then
        echo "$directory/ is already a worktree; leaving it alone"
        return
    fi
    if [ -e "$directory" ] && [ -n "$(ls -A "$directory")" ]; then
        echo "::error::$directory/ exists and is not a worktree. Move it away before loading the branch." >&2
        exit 1
    fi
    rm -rf "$directory"
    if git ls-remote --exit-code --heads origin "$branch" > /dev/null; then
        git fetch --quiet --depth=1 origin "+refs/heads/$branch:refs/remotes/origin/$branch"
        git worktree add --quiet -B "$branch" "$directory" "origin/$branch"
        echo "Loaded $branch at $(git -C "$directory" rev-parse --short HEAD)"
    else
        git worktree add --quiet --orphan -b "$branch" "$directory"
        echo "No $branch branch yet; starting one. The first refresh crawls from nothing."
    fi
}

save() {
    local summary=$1
    if [ ! -e "$directory/.git" ]; then
        echo "::error::$directory/ is not a worktree; run '$0 load' first." >&2
        exit 1
    fi
    git -C "$directory" add --all
    if git -C "$directory" diff --cached --quiet; then
        echo "Nothing observed changed; no commit."
        return
    fi
    local observed
    observed=$(jq -r .observed_at "$directory/network.json")
    git -C "$directory" \
        -c user.name='github-actions[bot]' \
        -c user.email='41898282+github-actions[bot]@users.noreply.github.com' \
        commit --quiet --file=- <<EOF
OBSERVE: Network state at $observed

$(cat "$summary")
EOF
    git -C "$directory" push --quiet origin "HEAD:refs/heads/$branch"
    echo "Pushed $branch at $(git -C "$directory" rev-parse --short HEAD)"
}

case "${1:-}" in
    load) load ;;
    save) save "${2:?usage: $0 save SUMMARY_FILE}" ;;
    *) echo "usage: $0 load | save SUMMARY_FILE" >&2; exit 2 ;;
esac
