# Build image for CI and releases: Rust, the Windows cross toolchain (MinGW, NSIS, Wine),
# the Linux packaging tools (rpm, dpkg, cargo-zigbuild) and node (for actions/checkout).
# Built and pushed by .github/workflows/builder-image.yml to the Forgejo container registry:
#   <forgejo-host>/<owner>/nmm-builder:latest
# It starts from the image Forgejo runners already use, so everything the workflows
# expect (node, git, curl, sudo) is there.
FROM ghcr.io/catthehacker/ubuntu:act-latest

ENV DEBIAN_FRONTEND=noninteractive \
    RUSTUP_HOME=/opt/rust/rustup \
    CARGO_HOME=/opt/rust/cargo \
    PATH=/opt/rust/cargo/bin:$PATH

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      gcc-mingw-w64-x86-64-posix binutils-mingw-w64-x86-64 nsis zip wine64 \
      rpm dpkg-dev python3-pip xdg-utils \
 && rm -rf /var/lib/apt/lists/*

RUN curl -sSf https://sh.rustup.rs | sh -s -- -y --profile default --default-toolchain stable \
 && rustup target add x86_64-pc-windows-gnu \
 && python3 -m pip install --no-cache-dir --break-system-packages cargo-zigbuild \
 && chmod -R a+rwX /opt/rust
