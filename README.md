# Guardian agent-linux

Linux Guardian agent (daemon, AppArmor, netlink, CEF overlay).

Depends on sibling checkout of `agent-common` at `../agent-common`.

Install: `scripts/install-agent.sh` (also published via the [versions feed](https://guardian-parental-controls.github.io/versions/feed.json)).

## Releases

Pushing a `vX.Y.Z` tag (or running the **Release** workflow) builds and attaches:

- `guardian-agent-x86_64-unknown-linux-gnu.tar.gz` + `.sha256`
- `guardian-agent-aarch64-unknown-linux-gnu.tar.gz` + `.sha256`
- `guardian-overlay-x86_64-unknown-linux-gnu.tar.gz` + `.sha256` (optional CEF helper)

Successful tag builds also dispatch the artifact URLs to the versions feed.
