#!/usr/bin/env bash
set -euo pipefail

APP="jqx"
REPO="sapirrior/jqx"

# Standard terminal colors (no brand theming, clean developer tool output)
BOLD='\033[1m'
DIM='\033[2m'
GREEN='\033[32m'
RED='\033[31m'
YELLOW='\033[33m'
CYAN='\033[36m'
NC='\033[0m' # Reset

# Disable colors when not in a terminal or NO_COLOR is set
if [ ! -t 1 ] || [ -n "${NO_COLOR:-}" ]; then
    BOLD=''
    DIM=''
    GREEN=''
    RED=''
    YELLOW=''
    CYAN=''
    NC=''
fi

usage() {
    cat <<EOF
jqx installer

Usage: install.sh [options]

Options:
    -h, --help              Display this help message
    -v, --version <version> Install a specific version (e.g. 0.1.0)
    -b, --binary <path>     Install from a local file instead of downloading
        --no-modify-path    Don't modify shell config files (.zshrc, .bashrc, etc.)

Examples:
    curl -fsSL https://raw.githubusercontent.com/${REPO}/main/install.sh | bash
    curl -fsSL https://raw.githubusercontent.com/${REPO}/main/install.sh | bash -s -- --version 0.1.0
    ./install.sh --binary ./target/release/jqx
EOF
}

requested_version=${VERSION:-}
no_modify_path=false
binary_path=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        -h|--help)
            usage
            exit 0
            ;;
        -v|--version)
            if [[ -n "${2:-}" ]]; then
                requested_version="$2"
                shift 2
            else
                echo -e "${RED}Error: --version requires a version argument${NC}"
                exit 1
            fi
            ;;
        -b|--binary)
            if [[ -n "${2:-}" ]]; then
                binary_path="$2"
                shift 2
            else
                echo -e "${RED}Error: --binary requires a path argument${NC}"
                exit 1
            fi
            ;;
        --no-modify-path)
            no_modify_path=true
            shift
            ;;
        *)
            echo -e "${YELLOW}Warning: Unknown option '$1'${NC}" >&2
            shift
            ;;
    esac
done

INSTALL_DIR="${HOME}/.jqx/bin"
mkdir -p "$INSTALL_DIR"

# Detect Termux environment
is_termux=false
if [ -n "${PREFIX:-}" ] && [ -d "${PREFIX}/bin" ]; then
    is_termux=true
fi

# Detect OS and architecture
if [ -n "$binary_path" ]; then
    if [ ! -f "$binary_path" ]; then
        echo -e "${RED}Error: File not found at ${binary_path}${NC}"
        exit 1
    fi
    specific_version="local"
else
    raw_os=$(uname -s)
    os=$(echo "$raw_os" | tr '[:upper:]' '[:lower:]')
    case "$raw_os" in
        Darwin*) os="darwin" ;;
        Linux*)  os="linux" ;;
        MINGW*|MSYS*|CYGWIN*) os="windows" ;;
    esac

    arch=$(uname -m)
    case "$arch" in
        aarch64|arm64) arch="arm64" ;;
        x86_64|amd64)  arch="x64" ;;
    esac

    # Check for Rosetta translation on macOS
    if [ "$os" = "darwin" ] && [ "$arch" = "x64" ]; then
        rosetta_flag=$(sysctl -n sysctl.proc_translated 2>/dev/null || echo 0)
        if [ "$rosetta_flag" = "1" ]; then
            arch="arm64"
        fi
    fi

    if [ "$is_termux" = "true" ] && [ "$arch" = "arm64" ]; then
        asset_name="jqx-android-arm64"
    else
        combo="$os-$arch"
        case "$combo" in
            linux-x64)    asset_name="jqx-linux-x64" ;;
            linux-arm64)  asset_name="jqx-linux-arm64" ;;
            darwin-x64)   asset_name="jqx-darwin-x64" ;;
            darwin-arm64) asset_name="jqx-darwin-arm64" ;;
            windows-x64)  asset_name="jqx-windows-x64.exe" ;;
            windows-arm64) asset_name="jqx-windows-arm64.exe" ;;
            *)
                echo -e "${RED}Error: Unsupported platform ${combo}${NC}"
                exit 1
                ;;
        esac
    fi

    # Resolve version
    if [ -z "$requested_version" ]; then
        latest_tag=$(curl -s "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null | sed -n 's/.*"tag_name": *"v\?\([^"]*\)".*/\1/p' | head -n 1)
        if [ -z "$latest_tag" ]; then
            specific_version="latest"
            download_url="https://github.com/${REPO}/releases/latest/download/${asset_name}"
            checksum_url="https://github.com/${REPO}/releases/latest/download/checksums.txt"
        else
            specific_version="$latest_tag"
            download_url="https://github.com/${REPO}/releases/download/v${specific_version}/${asset_name}"
            checksum_url="https://github.com/${REPO}/releases/download/v${specific_version}/checksums.txt"
        fi
    else
        specific_version="${requested_version#v}"
        download_url="https://github.com/${REPO}/releases/download/v${specific_version}/${asset_name}"
        checksum_url="https://github.com/${REPO}/releases/download/v${specific_version}/checksums.txt"

        http_code=$(curl -sI -o /dev/null -w "%{http_code}" "https://github.com/${REPO}/releases/tag/v${specific_version}")
        if [ "$http_code" = "404" ]; then
            echo -e "${RED}Error: Release v${specific_version} not found.${NC}"
            echo -e "${DIM}See available releases at: https://github.com/${REPO}/releases${NC}"
            exit 1
        fi
    fi
fi

print_footer() {
    echo ""
    echo -e "${BOLD}jqx${NC} — ${DIM}The post-modern JSON query executor${NC}"
    echo ""
    echo -e "${BOLD}Quickstart:${NC}"
    echo -e "  jqx --help"
    echo -e "  cat data.json | jqx 'select(.status == 200)'"
    echo ""
    echo -e "${DIM}Repository:${NC} https://github.com/${REPO}"
    echo ""
}

print_progress() {
    local bytes="$1"
    local length="$2"
    [ "$length" -gt 0 ] || return 0

    local width=35
    local percent=$(( bytes * 100 / length ))
    [ "$percent" -gt 100 ] && percent=100
    local on=$(( percent * width / 100 ))
    local off=$(( width - on ))

    local filled=$(printf "%*s" "$on" "")
    filled=${filled// /#}
    local empty=$(printf "%*s" "$off" "")
    empty=${empty// /-}

    local mb_down=$(awk "BEGIN {printf \"%.1f\", $bytes / 1048576}")
    local mb_total=$(awk "BEGIN {printf \"%.1f\", $length / 1048576}")

    printf "\r${DIM}Downloading: [${CYAN}%s${DIM}%s] %d%% (%s/%s MB)${NC}" \
        "$filled" "$empty" "$percent" "$mb_down" "$mb_total"
}

unbuffered_sed() {
    if echo | sed -u -e "" >/dev/null 2>&1; then
        sed -nu "$@"
    elif echo | sed -l -e "" >/dev/null 2>&1; then
        sed -nl "$@"
    else
        local pad="$(printf "\n%512s" "")"
        sed -ne "s/$/\\${pad}/" "$@"
    fi
}

download_file_with_progress() {
    local url="$1"
    local target_file="$2"

    local total_size
    total_size=$(curl -sI -L "$url" | tr -d '\r' | awk 'BEGIN{IGNORECASE=1} /^Content-Length:/ {print $2}' | tail -n 1)
    total_size=${total_size:-0}

    touch "$target_file"

    if [ "$total_size" -gt 0 ]; then
        curl -fL --progress-bar "$url" -o "$target_file" 2>&1 | \
        unbuffered_sed '
            s/^[ \t]*//
            s/[ \t]*$//
            /^$/d
            s/^#* *//
            s/%//
            /^[0-9]/ {
                p
            }
        ' | while read -r percent; do
            if [[ "$percent" =~ ^[0-9]+(\.[0-9]+)?$ ]]; then
                local current_bytes=$(awk "BEGIN {printf \"%.0f\", ($percent * $total_size) / 100}")
                print_progress "$current_bytes" "$total_size"
            fi
        done
        echo ""
    else
        echo -e "${DIM}Downloading jqx...${NC}"
        curl -fL "$url" -o "$target_file"
    fi
}

verify_checksum() {
    local file="$1"
    local expected_asset="$2"
    local checksum_file
    checksum_file=$(mktemp)

    if curl -sSfL "$checksum_url" -o "$checksum_file" 2>/dev/null; then
        local expected_hash
        expected_hash=$(grep "${expected_asset}" "$checksum_file" | awk '{print $1}' || echo "")
        rm -f "$checksum_file"

        if [ -n "$expected_hash" ]; then
            local actual_hash=""
            if command -v sha256sum >/dev/null 2>&1; then
                actual_hash=$(sha256sum "$file" | awk '{print $1}')
            elif command -v shasum >/dev/null 2>&1; then
                actual_hash=$(shasum -a 256 "$file" | awk '{print $1}')
            fi

            if [ -n "$actual_hash" ]; then
                if [ "$actual_hash" != "$expected_hash" ]; then
                    echo -e "${RED}Error: SHA-256 checksum verification failed!${NC}"
                    echo -e "Expected: ${expected_hash}"
                    echo -e "Actual:   ${actual_hash}"
                    rm -f "$file"
                    exit 1
                fi
                echo -e "${GREEN}✓ Verified SHA-256 checksum${NC}"
            fi
        fi
    else
        rm -f "$checksum_file"
    fi
}

check_existing_version() {
    local target_bin="${INSTALL_DIR}/jqx"
    if [ -x "$target_bin" ]; then
        local current
        current=$("$target_bin" --version 2>/dev/null || echo "")
        current=$(echo "$current" | tr -d 'v' | awk '{print $NF}')
        local target_clean=$(echo "$specific_version" | tr -d 'v')

        if [ "$specific_version" != "latest" ] && [ -n "$current" ] && [ "$current" = "$target_clean" ]; then
            echo -e "${GREEN}jqx version ${specific_version} is already installed at ${target_bin}.${NC}"
            print_footer
            exit 0
        fi
    fi
}

install_jqx() {
    local target_bin="${INSTALL_DIR}/jqx"

    if [ -n "$binary_path" ]; then
        echo -e "${DIM}Installing jqx from local file: ${NC}${binary_path}"
        cp "$binary_path" "$target_bin"
    else
        echo -e "${DIM}Installing jqx ${BOLD}${specific_version}${NC}${DIM} into ${BOLD}${INSTALL_DIR}/jqx${NC}"
        local tmp_file
        tmp_file=$(mktemp)
        download_file_with_progress "$download_url" "$tmp_file"
        verify_checksum "$tmp_file" "$asset_name"
        mv "$tmp_file" "$target_bin"
    fi

    chmod 755 "$target_bin"
    echo "$specific_version" > "${INSTALL_DIR}/.version"
    echo -e "${GREEN}Successfully installed jqx to ${BOLD}${target_bin}${NC}"
}

add_to_path() {
    local config_file="$1"
    local export_line="export PATH=\"${INSTALL_DIR}:\$PATH\""

    if [ -f "$config_file" ] && grep -Fq "${INSTALL_DIR}" "$config_file" 2>/dev/null; then
        return 0
    fi

    if [ -w "$config_file" ] || [ ! -e "$config_file" ]; then
        echo -e "\n# jqx" >> "$config_file"
        echo "$export_line" >> "$config_file"
        echo -e "${DIM}Added ${BOLD}~/.jqx/bin${NC}${DIM} to PATH in ${BOLD}${config_file}${NC}"
    else
        echo -e "${YELLOW}Notice: Add this line manually to your shell config:${NC}"
        echo -e "  ${export_line}"
    fi
}

configure_shell_path() {
    if [ "$no_modify_path" = "true" ]; then
        return 0
    fi

    if [[ ":$PATH:" == *":$INSTALL_DIR:"* ]]; then
        return 0
    fi

    local current_shell
    current_shell=$(basename "${SHELL:-bash}")

    case "$current_shell" in
        fish)
            local fish_conf="${XDG_CONFIG_HOME:-$HOME/.config}/fish/config.fish"
            if [ -w "$fish_conf" ] || [ ! -e "$fish_conf" ]; then
                mkdir -p "$(dirname "$fish_conf")"
                if ! grep -Fq "${INSTALL_DIR}" "$fish_conf" 2>/dev/null; then
                    echo -e "\n# jqx" >> "$fish_conf"
                    echo "fish_add_path ${INSTALL_DIR}" >> "$fish_conf"
                    echo -e "${DIM}Added ${BOLD}~/.jqx/bin${NC}${DIM} to PATH in ${BOLD}${fish_conf}${NC}"
                fi
            fi
            ;;
        zsh)
            for f in "${ZDOTDIR:-$HOME}/.zshrc" "$HOME/.zshrc" "$HOME/.profile"; do
                if [ -f "$f" ]; then
                    add_to_path "$f"
                    return 0
                fi
            done
            add_to_path "$HOME/.zshrc"
            ;;
        bash|*)
            for f in "$HOME/.bashrc" "$HOME/.bash_profile" "$HOME/.profile"; do
                if [ -f "$f" ]; then
                    add_to_path "$f"
                    return 0
                fi
            done
            add_to_path "$HOME/.bashrc"
            ;;
    esac
}

check_existing_version
install_jqx
configure_shell_path

if [ -n "${GITHUB_ACTIONS:-}" ] && [ "${GITHUB_ACTIONS}" = "true" ]; then
    echo "$INSTALL_DIR" >> "$GITHUB_PATH"
fi

print_footer
