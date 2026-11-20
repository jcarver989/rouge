set -euo pipefail

case "$ROUGE_CHANNEL" in preview|production) ;; *) exit 1 ;; esac
work=$(mktemp -d)
trap 'git worktree remove --force "$work/site" 2>/dev/null || true; rm -rf "$work"' EXIT
branch=rouge-pages-state
remote="refs/heads/$branch"
found=$(git ls-remote --refs origin "$remote")
if [[ -n $found ]]; then
    git fetch --no-tags --depth=1 origin "$remote"
    previous=$(git rev-parse FETCH_HEAD)
    git worktree add --detach "$work/site" "$previous"
else
    git init --quiet "$work/site"
fi
site="$work/site"
if [[ $ROUGE_CHANNEL == preview ]]; then
    rm -rf "$site/preview"
    mkdir "$site/preview"
    cp -R .rouge/out/web/. "$site/preview/"
    if [[ ! -e $site/index.html ]]; then
        printf '<!doctype html><meta charset="utf-8"><a href="./preview/">Open preview</a>\n' > "$site/index.html"
    fi
else
    find "$site" -mindepth 1 -maxdepth 1 ! -name .git ! -name preview -exec rm -rf {} +
    cp -R .rouge/out/web/. "$site/"
fi
rm -rf .rouge/out/pages
mkdir -p .rouge/out/pages
find "$site" -mindepth 1 -maxdepth 1 ! -name .git -exec cp -R {} .rouge/out/pages/ \;
git -C "$site" add --all
git -C "$site" -c user.name='github-actions[bot]' \
    -c user.email='41898282+github-actions[bot]@users.noreply.github.com' \
    commit --allow-empty -m "Rouge Pages $ROUGE_CHANNEL $GITHUB_SHA"
if [[ -n $found ]]; then
    git push origin "$(git -C "$site" rev-parse HEAD):$remote"
else
    git fetch --no-tags "$site" HEAD
    git push origin "FETCH_HEAD:$remote"
fi
