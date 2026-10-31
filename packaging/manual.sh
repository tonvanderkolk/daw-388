#!/bin/sh
# Print the user manual (docs/manual/manual.html) to a PDF with headless Chrome.
#
#   packaging/manual.sh <version> <out.pdf>
#
# Chrome is looked up in $CHROME, then in the usual places on macOS, Windows and Linux.
set -eu
version=$1
out=$2
here=$(cd "$(dirname "$0")" && pwd)
source="$here/../docs/manual/manual.html"

chrome=${CHROME:-}
if [ -z "$chrome" ]; then
    for candidate in \
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
        "/c/Program Files/Google/Chrome/Application/chrome.exe" \
        "/c/Program Files (x86)/Google/Chrome/Application/chrome.exe" \
        "$(command -v google-chrome 2>/dev/null || true)" \
        "$(command -v chromium 2>/dev/null || true)"; do
        if [ -n "$candidate" ] && [ -x "$candidate" ]; then
            chrome=$candidate
            break
        fi
    done
fi
if [ -z "$chrome" ]; then
    echo "manual.sh: Chrome not found; set CHROME" >&2
    exit 1
fi

# The version goes into a copy, next to the source so relative links keep working.
work=$(mktemp -d)
sed "s/@VERSION@/$version/g" "$source" >"$work/manual.html"
mkdir -p "$(dirname "$out")"
out_abs="$(cd "$(dirname "$out")" && pwd)/$(basename "$out")"
page="file://$work/manual.html"
case "$(uname -s)" in
    MINGW* | MSYS* | CYGWIN*)
        page="file:///$(cygpath -m "$work/manual.html")"
        out_abs=$(cygpath -w "$out_abs")
        ;;
esac
"$chrome" --headless --disable-gpu --no-pdf-header-footer --print-to-pdf="$out_abs" "$page" >/dev/null 2>&1
rm -rf "$work"
test -s "$out" || { echo "manual.sh: no PDF written" >&2; exit 1; }
echo "$out"
