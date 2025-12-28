# Updating

Keep argo-rs up to date with the latest features and fixes.

## Quick Update

Run the install script again to update to the latest version:

```bash
curl -sSL https://raw.githubusercontent.com/stefanodecillis/argo-rs/main/install.sh | bash
```

The script will:
1. Download the latest release
2. Replace the existing binary
3. Preserve your configuration and credentials

## Check Current Version

```bash
argo --version
```

## Update from Source

If you built from source:

```bash
cd argo-rs
git pull origin main
cargo build --release
cp target/release/argo ~/.local/bin/
```
