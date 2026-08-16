# Handy

Handy is a local-first desktop app for starting, stopping, and watching all the services used by your development projects.

This first implementation includes:

- Projects with any shell command and a relative working directory
- Detection of `package.json` scripts and standard Docker Compose files
- Reusable nested groups with cycle validation and deduplicated commands
- Project, group, and individual Run/Stop controls
- Shared-service ownership: stopping one group keeps commands needed by another active group
- Per-command and merged live logs with smart scroll following and a 32 MiB session limit
- Optional status commands that detect services started or stopped outside Handy
- Versioned JSON persistence with an automatic backup; no database or cloud service
- Linux, macOS, and Windows process-tree termination

## Development

Requirements: Node.js 22+, pnpm 10+, Rust, and the [Tauri 2 system prerequisites](https://v2.tauri.app/start/prerequisites/).

On Ubuntu/Debian, install the native packages once:

```sh
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev libdbus-1-dev pkg-config
```

Then run:

```sh
pnpm install
pnpm tauri dev
```

On Fedora / Red Hat style systems, install the native packages once:

```sh
sudo dnf install webkit2gtk4.1-devel gcc-c++ curl wget file openssl-devel libappindicator-gtk3-devel librsvg2-devel xdotool dbus-devel
```

Checks:

```sh
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features
cargo test --manifest-path src-tauri/Cargo.toml
```

The `--no-default-features` test runs the portable configuration and graph tests without linking the OS desktop libraries.

## Linux installers

Handy can be packaged as native Linux installers with Tauri.

Build both Linux package formats:

```sh
pnpm bundle
```

Build only a Debian package:

```sh
pnpm bundle:deb
```

Build only an RPM package:

```sh
pnpm bundle:rpm
```

The generated installers are written under `src-tauri/target/release/bundle/`, including:

- `deb/` for Debian and Ubuntu
- `rpm/` for Fedora, RHEL, Rocky, AlmaLinux, and similar distributions

Install locally with:

```sh
sudo dpkg -i src-tauri/target/release/bundle/deb/*.deb
sudo rpm -i src-tauri/target/release/bundle/rpm/*.rpm
```

For widest Linux compatibility, build on an older supported base such as Ubuntu 22.04 or Debian 12 so the resulting binaries do not require a newer glibc than your target systems provide. This follows Tauri's Linux distribution guidance.

## Storage and trust

Configuration is stored as `config.v1.json` under Tauri's application data directory. Commands are executed through the user's login shell and have the same permissions as Handy, so only add or import commands you trust.

## External service status

Set a command's optional status command when you want Handy to recognize a service started from another terminal or before Handy opened. Handy runs the check from the command's working directory when it starts, then every 15 seconds.

- Exit code `0` means the service is running.
- Any other exit code means the service is stopped.
- Handy stops a service it detected outside the app only when you configure a stop command.
- Quitting Handy leaves externally started services running.

Docker Compose suggestions include status and stop commands. Handy treats the Compose command as running while at least one container reports a running state.

Handy keeps direct child-process tracking for commands without a status command. Process-name matching is not used because unrelated programs can share the same executable or command line.

## Next slices

Selective `.handy.json` import/export, tray/quit behavior, single-instance handling, signed installers, and GitHub-based updates remain on the v1 roadmap.

## License

MIT
