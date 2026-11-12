#!/usr/bin/env bash
# Fetch the pinned np into apps/shell/np/ so the bundler can ship it inside the installer.
#
# The hash in packaging/np-pin.json is not decorative: an installer that carries a half-downloaded or
# swapped-out backend would index somebody's notes with it, and nothing downstream could tell. So the
# download is verified before it is unpacked, and the marker file records which archive this folder holds.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
pin="$root/packaging/np-pin.json"
dest="$root/apps/shell/np"

os=$(uname -s)
arch=$(uname -m)
case "$os/$arch" in
  Linux/x86_64) triple="x86_64-unknown-linux-gnu" ;;
  Darwin/*)
    echo "There is no np release asset for macOS: build it with" >&2
    echo "  cargo install --git https://github.com/Ex-Bot-01/note-processor -p np-cli --locked" >&2
    echo "and point NP_PATH at it. Skipping the bundle step." >&2
    exit 0
    ;;
  *)
    echo "no pinned np asset for $os/$arch — add it to $pin if a release exists" >&2
    exit 1
    ;;
esac

read -r archive binary sha256 base <<<"$(python3 - "$pin" "$triple" <<'PY'
import json, sys
conf = json.load(open(sys.argv[1]))
asset = conf["assets"][sys.argv[2]]
print(asset["archive"], asset["binary"], asset["sha256"], conf["release"])
PY
)"

marker="$dest/.fetched-from"
if [ -f "$marker" ] && [ -f "$dest/$binary" ] && [ "$(cat "$marker")" = "$sha256" ]; then
  echo "np already fetched from $archive"
  exit 0
fi

mkdir -p "$dest"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
url="$base/$archive"
echo "downloading $url"
curl -fsSL --retry 3 -o "$tmp/$archive" "$url"

echo "$sha256  $tmp/$archive" | sha256sum -c - || {
  echo "$archive did not match the hash in $pin — refusing to bundle it" >&2
  exit 1
}

case "$archive" in
  *.tar.gz) tar xzf "$tmp/$archive" -C "$tmp" "$binary" ;;
  *.zip) unzip -qo "$tmp/$archive" "$binary" -d "$tmp" ;;
  *) echo "don't know how to unpack $archive" >&2; exit 1 ;;
esac

install -m 0755 "$tmp/$binary" "$dest/$binary"
echo "$sha256" >"$marker"
echo "np is at $dest/$binary, from $archive"
