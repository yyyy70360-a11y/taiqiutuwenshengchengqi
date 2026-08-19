# Windows tunnel release resources

The Windows release build may bundle these local-only files:

- `billiards_tunnel_ed25519`: restricted SSH key used only for the cloud API tunnel.
- `ssh.exe`: Windows OpenSSH client used when the target machine has no system SSH client.

Both files are ignored by Git. Obtain them through the release team's secure channel before producing public installers. The server account must deny shell access and restrict forwarding to `127.0.0.1:38123`.
