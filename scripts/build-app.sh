#!/bin/bash
# Empaqueta Ariadna.app, un .dmg y un .zip en un Mac.
# Lo usa también .github/workflows/release.yml.
# No firma con Developer ID ni notariza salvo que estén las cinco variables
# APPLE_CERT_P12 (base64), APPLE_CERT_PASSWORD, APPLE_ID, APPLE_TEAM_ID y APPLE_APP_PASSWORD.
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "scripts/build-app.sh solo funciona en macOS (hace falta codesign, hdiutil y Xcode)." >&2
    exit 1
fi

root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
cd "$root"

version_from_ref() {
    local ref="${1:-}"
    if [[ "$ref" == refs/tags/v* ]]; then
        printf '%s' "${ref#refs/tags/v}"
        return
    fi
    if git describe --tags --exact-match >/dev/null 2>&1; then
        local tag
        tag=$(git describe --tags --exact-match)
        if [[ "$tag" == v* ]]; then
            printf '%s' "${tag#v}"
            return
        fi
    fi
    printf '0.0.0-%s' "$(git rev-parse --short HEAD)"
}

VERSION="${ARIADNA_VERSION:-$(version_from_ref "${GITHUB_REF:-}")}"
# CFBundleVersion solo admite enteros separados por puntos.
if [[ "$VERSION" =~ ^[0-9]+(\.[0-9]+){0,2}$ ]]; then
    BUILD_VERSION="$VERSION"
else
    BUILD_VERSION="0.0.0.$(git rev-list --count HEAD)"
fi

echo "Versión de empaquetado: $VERSION (CFBundleVersion $BUILD_VERSION)"

echo "Compilando binario universal (arm64 + x86_64), configuración release…"
swift build -c release --product Ariadna --arch arm64 --arch x86_64

binary=""
for candidate in \
    ".build/apple/Products/Release/Ariadna" \
    ".build/apple/Products/Release/App" \
    ".build/release/Ariadna"
do
    if [[ -f "$candidate" && -x "$candidate" ]]; then
        binary="$candidate"
        break
    fi
done
if [[ -z "$binary" ]]; then
    binary=$(find .build -type f -name Ariadna -perm -111 ! -path '*.dSYM/*' | head -n 1 || true)
fi
if [[ -z "$binary" || ! -f "$binary" ]]; then
    echo "No encuentro el ejecutable Ariadna tras swift build." >&2
    exit 1
fi

echo "Binario: $binary"
lipo -info "$binary"
if ! lipo -info "$binary" | grep -q "arm64" || ! lipo -info "$binary" | grep -q "x86_64"; then
    echo "El binario no es universal (arm64 y x86_64)." >&2
    exit 1
fi

dist="$root/dist"
rm -rf "$dist"
mkdir -p "$dist"
app="$dist/Ariadna.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$binary" "$app/Contents/MacOS/Ariadna"
chmod 755 "$app/Contents/MacOS/Ariadna"
cp "$root/Sources/App/Info.plist" "$app/Contents/Info.plist"

plist_set() {
    /usr/libexec/PlistBuddy -c "Set :$1 $2" "$app/Contents/Info.plist"
}
plist_set CFBundleShortVersionString "$VERSION"
plist_set CFBundleVersion "$BUILD_VERSION"
plist_set CFBundleIdentifier "es.javiermartinez.ariadna"
plist_set CFBundleExecutable "Ariadna"
plist_set LSMinimumSystemVersion "15.0"

# Sin esta firma, un Mac con Apple Silicon se niega a abrir el binario.
echo "Firma ad-hoc (codesign -s -)…"
codesign --force --deep --sign - "$app"
codesign --verify --deep --strict "$app"

have_apple_secrets() {
    [[ -n "${APPLE_CERT_P12:-}" && -n "${APPLE_CERT_PASSWORD:-}" && -n "${APPLE_ID:-}" && -n "${APPLE_TEAM_ID:-}" && -n "${APPLE_APP_PASSWORD:-}" ]]
}

KEYCHAIN=""
CERT_P12=""
cleanup() {
    if [[ -n "$KEYCHAIN" ]]; then
        security delete-keychain "$KEYCHAIN" >/dev/null 2>&1 || true
    fi
    if [[ -n "$CERT_P12" && -f "$CERT_P12" ]]; then
        rm -f "$CERT_P12"
    fi
}
trap cleanup EXIT

if have_apple_secrets; then
    echo "Secrets de Apple presentes: firma Developer ID y notarización."
    work=$(mktemp -d)
    CERT_P12="$work/cert.p12"
    KEYCHAIN="$work/ariadna.keychain-db"
    kc_pass=$(openssl rand -base64 24)
    python3 -c 'import sys,base64; sys.stdout.buffer.write(base64.b64decode(sys.stdin.read()))' <<<"$APPLE_CERT_P12" >"$CERT_P12"
    security create-keychain -p "$kc_pass" "$KEYCHAIN"
    security set-keychain-settings -lut 21600 "$KEYCHAIN"
    security unlock-keychain -p "$kc_pass" "$KEYCHAIN"
    security import "$CERT_P12" -k "$KEYCHAIN" -P "$APPLE_CERT_PASSWORD" -T /usr/bin/codesign -T /usr/bin/security >/dev/null
    security set-key-partition-list -S apple-tool:,apple: -s -k "$kc_pass" "$KEYCHAIN" >/dev/null
    security list-keychains -d user -s "$KEYCHAIN" $(security list-keychains -d user | sed 's/"//g')
    identity=$(security find-identity -v -p codesigning "$KEYCHAIN" | sed -n 's/.*"\(Developer ID Application:[^"]*\)".*/\1/p' | head -n 1)
    if [[ -z "$identity" ]]; then
        echo "El .p12 no tiene una identidad «Developer ID Application»." >&2
        exit 1
    fi
    codesign --force --deep --options runtime --timestamp --sign "$identity" "$app"
    codesign --verify --deep --strict --verbose=2 "$app"
else
    echo "Firma Developer ID y notarización desactivadas: faltan uno o más secrets APPLE_CERT_P12, APPLE_CERT_PASSWORD, APPLE_ID, APPLE_TEAM_ID, APPLE_APP_PASSWORD."
fi

archive_base="Ariadna-${VERSION}"
ditto -c -k --keepParent "$app" "$dist/${archive_base}.zip"
hdiutil create -volname "Ariadna" -srcfolder "$app" -ov -format UDZO "$dist/${archive_base}.dmg" >/dev/null

if have_apple_secrets; then
    echo "Enviando a notarización…"
    xcrun notarytool submit "$dist/${archive_base}.zip" \
        --apple-id "$APPLE_ID" \
        --team-id "$APPLE_TEAM_ID" \
        --password "$APPLE_APP_PASSWORD" \
        --wait
    xcrun stapler staple "$app"
    rm -f "$dist/${archive_base}.zip" "$dist/${archive_base}.dmg"
    ditto -c -k --keepParent "$app" "$dist/${archive_base}.zip"
    hdiutil create -volname "Ariadna" -srcfolder "$app" -ov -format UDZO "$dist/${archive_base}.dmg" >/dev/null
    xcrun notarytool submit "$dist/${archive_base}.dmg" \
        --apple-id "$APPLE_ID" \
        --team-id "$APPLE_TEAM_ID" \
        --password "$APPLE_APP_PASSWORD" \
        --wait
    xcrun stapler staple "$dist/${archive_base}.dmg" || true
fi

echo "Listo:"
ls -lh "$dist/${archive_base}.zip" "$dist/${archive_base}.dmg"
echo "VERSION=$VERSION"
echo "BUILD_VERSION=$BUILD_VERSION"
