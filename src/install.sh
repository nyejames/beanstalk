#!/usr/bin/env sh

set -eu

REPO="${BST_REPO:-nyejames/beanstalk}"
BIN_NAME="bean"
VERSION="${BST_VERSION:-latest}"
BIN_DIR="${BIN_DIR:-$HOME/.local/bin}"

say() {
    printf '%s\n' "$1"
}

fail() {
    printf 'error: %s\n' "$1" >&2
    exit 1
}

need_command() {
    command -v "$1" >/dev/null 2>&1 || fail "required command not found: $1"
}

detect_target() {
    os="$(uname -s)"
    arch="$(uname -m)"

    case "$os" in
        Linux)
            case "$arch" in
                x86_64 | amd64)
                    TARGET="x86_64-unknown-linux-gnu"
                    ARCHIVE_EXT="tar.gz"
                    BINARY_NAME="$BIN_NAME"
                    ;;
                *)
                    fail "unsupported Linux architecture: $arch"
                    ;;
            esac
            ;;

        Darwin)
            case "$arch" in
                arm64 | aarch64)
                    TARGET="aarch64-apple-darwin"
                    ARCHIVE_EXT="tar.gz"
                    BINARY_NAME="$BIN_NAME"
                    ;;
                x86_64 | amd64)
                    TARGET="x86_64-apple-darwin"
                    ARCHIVE_EXT="tar.gz"
                    BINARY_NAME="$BIN_NAME"
                    ;;
                *)
                    fail "unsupported macOS architecture: $arch"
                    ;;
            esac
            ;;

        MINGW* | MSYS* | CYGWIN*)
            case "$arch" in
                x86_64 | amd64)
                    TARGET="x86_64-pc-windows-msvc"
                    ARCHIVE_EXT="zip"
                    BINARY_NAME="$BIN_NAME.exe"
                    ;;
                *)
                    fail "unsupported Windows architecture for bash installer: $arch"
                    ;;
            esac
            ;;

        *)
            fail "unsupported OS: $os"
            ;;
    esac
}

resolve_version() {
    if [ "$VERSION" != "latest" ]; then
        return
    fi

    need_command curl

    # GitHub's /latest endpoint ignores prereleases, so query the releases list.
    VERSION="$(
        curl -fsSL "https://api.github.com/repos/$REPO/releases?per_page=1" \
            | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
            | head -n 1
    )"

    [ -n "$VERSION" ] || fail "could not resolve latest release version"
}

download_file() {
    url="$1"
    output="$2"

    say "Downloading $url"
    curl -fL --proto '=https' --tlsv1.2 "$url" -o "$output"
}

verify_checksum() {
    checksums_file="$1"
    archive_file="$2"
    archive_name="$3"

    expected="$(
        awk -v name="$archive_name" '$2 == name { print $1 }' "$checksums_file"
    )"

    [ -n "$expected" ] || fail "checksum not found for $archive_name"

    if command -v sha256sum >/dev/null 2>&1; then
        printf '%s  %s\n' "$expected" "$archive_file" | sha256sum -c - >/dev/null
        return
    fi

    if command -v shasum >/dev/null 2>&1; then
        actual="$(shasum -a 256 "$archive_file" | awk '{ print $1 }')"
        [ "$actual" = "$expected" ] || fail "checksum mismatch for $archive_name"
        return
    fi

    fail "could not verify checksum: sha256sum or shasum is required"
}

extract_archive() {
    archive_file="$1"
    extract_dir="$2"

    mkdir -p "$extract_dir"

    case "$ARCHIVE_EXT" in
        tar.gz)
            need_command tar
            tar -xzf "$archive_file" -C "$extract_dir"
            ;;

        zip)
            need_command unzip
            unzip -q "$archive_file" -d "$extract_dir"
            ;;

        *)
            fail "unsupported archive type: $ARCHIVE_EXT"
            ;;
    esac
}

install_binary() {
    extract_dir="$1"

    found_binary="$(
        find "$extract_dir" -type f -name "$BINARY_NAME" | head -n 1
    )"

    [ -n "$found_binary" ] || fail "could not find $BINARY_NAME in archive"

    mkdir -p "$BIN_DIR"
    cp "$found_binary" "$BIN_DIR/$BINARY_NAME"
    chmod +x "$BIN_DIR/$BINARY_NAME" 2>/dev/null || true
}

check_path() {
    case ":$PATH:" in
        *":$BIN_DIR:"*)
            ;;
        *)
            say ""
            say "Installed to $BIN_DIR, but that directory is not in PATH."
            say "Add this to your shell profile:"
            say ""
            say "  export PATH=\"\$PATH:$BIN_DIR\""
            ;;
    esac
}

main() {
    need_command curl
    need_command awk
    need_command sed
    need_command find

    detect_target
    resolve_version

    archive_name="$BIN_NAME-$VERSION-$TARGET.$ARCHIVE_EXT"
    release_base_url="https://github.com/$REPO/releases/download/$VERSION"

    temp_dir="$(mktemp -d 2>/dev/null || mktemp -d -t beanstalk-install)"
    trap 'rm -rf "$temp_dir"' EXIT INT TERM

    archive_file="$temp_dir/$archive_name"
    checksums_file="$temp_dir/SHA256SUMS"
    extract_dir="$temp_dir/extract"

    say "Installing Beanstalk CLI"
    say "Version: $VERSION"
    say "Target:  $TARGET"
    say "Binary:  $BINARY_NAME"
    say "Install: $BIN_DIR/$BINARY_NAME"
    say ""

    download_file "$release_base_url/$archive_name" "$archive_file"
    download_file "$release_base_url/SHA256SUMS" "$checksums_file"

    verify_checksum "$checksums_file" "$archive_file" "$archive_name"
    extract_archive "$archive_file" "$extract_dir"
    install_binary "$extract_dir"

    say ""
    say "Installed:"
    "$BIN_DIR/$BINARY_NAME" --version || fail "installed binary failed to run"

    check_path
}

main "$@"