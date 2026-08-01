# Handy

Handy is a local-first desktop app for starting, stopping, and watching all the services used by your development projects.

This first implementation includes:

- Projects with any shell command and a relative working directory
- Detection of `package.json` scripts and standard Docker Compose files
- Reusable nested groups with cycle validation and deduplicated commands
- Project, group, and individual Run/Stop controls
- Shared-service ownership: stopping one group keeps commands needed by another active group
- Per-command and merged live logs with a 32 MiB in-memory session limit
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

Checks:

```sh
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features
cargo test --manifest-path src-tauri/Cargo.toml
```

The `--no-default-features` test runs the portable configuration and graph tests without linking the OS desktop libraries.

## Storage and trust

Configuration is stored as `config.v1.json` under Tauri's application data directory. Commands are executed through the user's login shell and have the same permissions as Handy, so only add or import commands you trust.

## Next slices

Selective `.handy.json` import/export, tray/quit behavior, single-instance handling, signed installers, and GitHub-based updates remain on the v1 roadmap.

## License

MIT

