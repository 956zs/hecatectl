#!/usr/bin/env bash
# Build and install hecatectl and hecate-gui for the current user.
#
#   scripts/install.sh              build (release) and install
#   scripts/install.sh --dry-run    print what would be done, change nothing
#   scripts/install.sh --uninstall  remove what the install put in place
#
# Installs to $HOME/.local/bin and $XDG_DATA_HOME/applications. Never runs sudo: the udev rule
# command is printed for you to run.
set -euo pipefail

repo=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
bin_dir="$HOME/.local/bin"
data_dir="${XDG_DATA_HOME:-$HOME/.local/share}"
apps_dir="$data_dir/applications"
icons_dir="$data_dir/icons/hicolor/scalable/apps"
desktop_src="$repo/crates/hecate-gui/data/hecate-gui.desktop"
icon_src="$repo/crates/hecate-gui/data/hecate-gui.svg"
rule_src="$repo/udev/70-hecatectl.rules"
rule_dst="/etc/udev/rules.d/70-hecatectl.rules"
bins=(hecatectl hecate-gui)

dry_run=0
uninstall=0

usage() {
    sed -n '2,9s/^# \{0,1\}//p' "${BASH_SOURCE[0]}"
}

for arg in "$@"; do
    case "$arg" in
        --dry-run) dry_run=1 ;;
        --uninstall) uninstall=1 ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            echo "install.sh: unknown option '$arg'" >&2
            usage >&2
            exit 2
            ;;
    esac
done

die() {
    echo "install.sh: $*" >&2
    exit 1
}

((EUID != 0)) || die "run this as your user, not root: it installs into \$HOME and never needs sudo"

# Runs a command, or only prints it with --dry-run.
run() {
    if ((dry_run)); then
        printf 'would run:'
        printf ' %q' "$@"
        printf '\n'
    else
        "$@"
    fi
}

# Copies $1 to $2 with mode $3 unless $2 already has the same content.
put() {
    local src=$1 dst=$2 mode=$3
    if [[ -f $dst ]] && cmp -s -- "$src" "$dst"; then
        echo "up to date: $dst"
    else
        run install -D -m "$mode" -- "$src" "$dst"
    fi
}

remove() {
    if [[ -e $1 ]]; then
        run rm -f -- "$1"
    else
        echo "not installed: $1"
    fi
}

refresh_desktop_database() {
    if command -v update-desktop-database >/dev/null && [[ -d $apps_dir ]]; then
        run update-desktop-database -q "$apps_dir"
    fi
}

# A stale icon-theme.cache hides newly installed icons from GTK and Qt lookups, so rebuild it
# whenever one exists in the user hicolor theme.
refresh_icon_cache() {
    local theme="$data_dir/icons/hicolor"
    if [[ -f $theme/icon-theme.cache ]] && command -v gtk-update-icon-cache >/dev/null; then
        run gtk-update-icon-cache -q -f -t "$theme"
    fi
}

rule_installed() {
    [[ -f $rule_dst ]] && cmp -s -- "$rule_src" "$rule_dst"
}

check_tools() {
    command -v cargo >/dev/null ||
        die "cargo not found: install Rust 1.87 or newer (https://rustup.rs or your distribution's rust package)"
    local qmake=${QMAKE:-qmake6}
    command -v "$qmake" >/dev/null ||
        die "$qmake not found: install Qt 6 (Arch: qt6-base qt6-declarative), or set QMAKE to a Qt 6 qmake"
    local qml_dir
    qml_dir=$("$qmake" -query QT_INSTALL_QML) || die "$qmake -query failed; is it a Qt 6 qmake?"
    [[ -d $qml_dir/QtQuick/Controls/Material ]] ||
        die "Qt Quick Controls (Material style) not found under $qml_dir: install qt6-declarative"
}

target_dir() {
    # Honours CARGO_TARGET_DIR and build.target-dir. --locked: never touch Cargo.lock (also in --dry-run).
    local dir
    dir=$(cargo metadata --locked --format-version 1 --no-deps --manifest-path "$repo/Cargo.toml" 2>/dev/null |
        sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p') || true
    if [[ -z $dir ]]; then
        dir=${CARGO_TARGET_DIR:-$repo/target}
        [[ $dir == /* ]] || dir="$PWD/$dir"
    fi
    printf '%s\n' "$dir"
}

do_install() {
    check_tools
    local nice_cmd=(nice -n 19)
    if command -v ionice >/dev/null; then
        nice_cmd+=(ionice -c3)
    fi
    run "${nice_cmd[@]}" cargo build --release --locked --manifest-path "$repo/Cargo.toml" -p hecatectl -p hecate-gui

    local target
    target=$(target_dir)
    [[ -n $target ]] || die "could not find cargo's target directory"
    for b in "${bins[@]}"; do
        if ((!dry_run)) && [[ ! -x $target/release/$b ]]; then
            die "$target/release/$b was not built"
        fi
        put "$target/release/$b" "$bin_dir/$b" 755
    done
    put "$desktop_src" "$apps_dir/hecate-gui.desktop" 644
    if [[ -f $icon_src ]]; then
        put "$icon_src" "$icons_dir/hecate-gui.svg" 644
    fi
    refresh_desktop_database
    refresh_icon_cache

    case ":$PATH:" in
        *":$bin_dir:"*) ;;
        *) echo "note: $bin_dir is not in PATH; add it so the shell and the desktop launcher find hecate-gui" ;;
    esac

    if rule_installed; then
        echo "udev rule already installed: $rule_dst"
    else
        cat <<EOF

To use the mouse without root, install the udev rule (run this yourself):
  sudo install -m 0644 $(printf '%q' "$rule_src") $rule_dst
  sudo udevadm control --reload-rules && sudo udevadm trigger --subsystem-match=hidraw
then unplug and replug the receiver or cable.
EOF
    fi
}

do_uninstall() {
    for b in "${bins[@]}"; do
        remove "$bin_dir/$b"
    done
    remove "$apps_dir/hecate-gui.desktop"
    remove "$icons_dir/hecate-gui.svg"
    refresh_desktop_database
    refresh_icon_cache
    echo "kept: your settings in ${XDG_CONFIG_HOME:-$HOME/.config}/hecate-gui (delete by hand if wanted)"
    if [[ -f $rule_dst ]]; then
        cat <<EOF

The udev rule is still installed. To remove it (run this yourself):
  sudo rm $rule_dst
  sudo udevadm control --reload-rules
EOF
    fi
}

if ((uninstall)); then
    do_uninstall
else
    do_install
fi
if ((dry_run)); then
    echo "dry run: nothing was changed"
fi
