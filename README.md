# swayview

A lightweight, Mission Control–style workspace overview for [Sway](https://swaywm.org/), with drag-and-drop window management, workspace reordering and multi-monitor support.

This fork extends [agejevasv/swayview](https://github.com/agejevasv/swayview) with additional workspace management features while preserving its native Wayland interface and minimal architecture.

Each monitor displays its own workspaces, with windows represented at their relative positions and sizes. The overview reads the layout directly from Sway over IPC and requires no background daemon. Live window thumbnails are available on Sway 1.12 and later when supported by the compositor; older versions use labelled window previews.

<!-- Add an updated screenshot of this fork here. -->

## Features

- **Multi-monitor overview:** see the workspaces and windows belonging to each display.
- **Window drag and drop:** move individual windows between workspaces and monitors without leaving the overview.
- **Workspace reordering:** drag an entire workspace to a new position. The actual workspace order in Sway changes, not just the preview.
- **Workspace transfer:** move a workspace and its windows to another monitor using drag and drop.
- **Super + drag:** grab a workspace even when its preview is completely occupied by windows.
- **Improved window stacking:** smaller window previews are drawn above larger ones, preserving their positions and dimensions. Windows of equal area retain their original stacking order.
- **Minimal interface:** workspace numbers and names are hidden; application and output labels remain visible.
- **Visual drop targets:** an outline identifies a workspace during dragging, and an insertion marker indicates the new workspace position.
- **Keyboard navigation:** select windows and switch workspaces without a mouse.
- **Optional touchpad integration:** IPC hooks allow an external workspace-swipe helper to navigate workspaces on the monitor under the pointer.

## Installation

The original overview supports Sway 1.4 or later. The extended workspace management features have been tested on Sway 1.10.1. Live thumbnails require Sway 1.12 or later.

Build this fork from source using Rust stable (the extended version has been tested with Rust 1.100.0), a C linker, `pkg-config` and the development files for `libxkbcommon`.

### Debian / Ubuntu

```sh
sudo apt install --no-install-recommends \
    git curl build-essential pkgconf libxkbcommon-dev
```

If Rust is not already installed, follow the instructions at [rustup.rs](https://rustup.rs/) to install the stable toolchain.

### Build from source

```sh
git clone https://github.com/sabx-ru/swayview.git
cd swayview
cargo test --locked
cargo build --release --locked
```

Install for the current user, without root privileges:

```sh
install -Dm755 target/release/swayview "$HOME/.local/bin/swayview"
```

Alternatively, install system-wide after a successful build:

```sh
sudo make install
```

The Makefile supports `PREFIX` and `DESTDIR`.

**Note:** Use the source build for this fork for now. The inherited `install.sh` and upstream distribution packages still install the original project, not this modified version.

## Sway setup

Add the following binding to your Sway configuration (usually `~/.config/sway/config`). It assumes `$mod` is defined as `Mod4` and swayview is installed in `~/.local/bin`.

```conf
bindsym --no-repeat $mod+Tab exec sh -c 'pkill -x swayview || exec "$HOME/.local/bin/swayview"'
```

Press **Super + Tab** to open or close the overview. Adjust the executable path if you installed it elsewhere. Reload your Sway configuration with `swaymsg reload`.

### Optional touchpad gestures

On a Sway version supporting `bindgesture`, the following bindings open and close the overview using three-finger swipes:

```conf
bindgesture swipe:3:up exec sh -c 'pkill -x swayview || exec "$HOME/.local/bin/swayview"'
bindgesture swipe:3:down exec pkill -x swayview
```

The fork also listens for Sway IPC tick payloads `swayview-workspace:left` and `swayview-workspace:right`. When it receives one of these events, it invokes `~/.local/bin/sway-workspace-swipe <direction> --output <output>` for the monitor under the pointer.

The external `sway-workspace-swipe` helper is not yet included in this repository. Horizontal swipe navigation requires a compatible helper; it is not enabled by installing swayview alone.

## Controls

| Input | Action |
| --- | --- |
| Left click on a window | Focus the window and close the overview |
| Drag a window | Move the window to another workspace or monitor |
| Drag an unoccupied area of a workspace preview | Reorder or transfer the entire workspace |
| Super + left drag within a workspace preview | Reorder or transfer the entire workspace, even over a window |
| Arrow keys / H, J, K, L | Move the selection |
| Tab / Shift + Tab | Select the next / previous window |
| Enter | Focus the selected window |
| 1–9 / 0 | Switch to workspace 1–9 / 10 |
| Escape during a workspace drag | Cancel the workspace drag |
| Escape / click outside a window without dragging | Close the overview |

### Moving workspaces

Drag a workspace toward the desired location, then release on the left or right side of a target workspace to insert before or after it. For cross-monitor transfers, move the pointer onto the destination monitor before choosing the insertion point.

The overview stays open after a workspace move. A workspace's windows move together using Sway's native workspace commands; Sway may adjust window geometry for the new output.

### Current limitations

- Workspace reordering supports plain positive numeric workspace names, such as `1`, `2` and `3`. Named workspaces and conflicting numbers are not supported by the reorder operation.
- Reordering may change workspace numbers. Number-based Sway bindings and workspace assignments can therefore refer to a different workspace after a move.
- Completely empty or sticky-only workspaces cannot be transferred across outputs. Sway may remove an empty workspace or create a replacement when moving workspaces between monitors.
- Workspace reordering uses multiple IPC commands. It attempts to restore the original state on failure, but the operation is not atomic.
- Smaller previews remain at their original relative locations, so overlapping previews can still obscure one another. This is not an automatic non-overlapping window layout.

## Configuration

Create `~/.config/swayview/config.yaml` (or `$XDG_CONFIG_HOME/swayview/config.yaml`) and include only the settings you want to override. Configuration warnings are printed to stderr.

Example configuration with the original default palette:

```yaml
thumbnails: true
fonts:
  app:
    family: "sans-serif"
    size: 14
  title:
    family: "sans-serif"
    size: 12
colors:
  backdrop: "#101216e0"
  output_name: "#8a93a5"
  workspace:
    fill: "#16181d"
  window:
    normal:
      border: "#333333"
      background: "#222222"
      text: "#888888"
    selected:
      border: "#4c7899"
      background: "#285577"
      text: "#ffffff"
    urgent:
      border: "#2f343a"
      background: "#900000"
      text: "#ffffff"
```

Window colors default to the `client.*` colors in the Sway configuration. Override `colors.window.selected.border` to customize the drag outline and insertion marker. Set `thumbnails: false` to use labelled previews instead of live window contents.

The original `colors.workspace.label`, `selected` and `urgent` settings remain accepted for compatibility, but workspace labels are disabled in this fork.

## Development

```sh
cargo test --locked
cargo build --release --locked
```

The source includes tests for window stacking, hit testing and workspace reordering. The overview is implemented in Rust and communicates with Sway via IPC and Wayland; no additional desktop environment or background service is required.

## Credits and license

Based on the original [swayview](https://github.com/agejevasv/swayview) by Viktoras Agejevas. This fork is maintained by [sabx-ru](https://github.com/sabx-ru).

Released under the [MIT License](LICENSE). The original copyright and license notice are retained.
