# Ubuntu 22.04 amd64 is the supported Linux runtime baseline. The base image is
# digest-pinned; signed Ubuntu apt repositories provide the generic build tools.
# Their resolved versions are written into the package build metadata because
# the live apt indexes are not a byte-reproducible build input across time.
FROM ubuntu:22.04@sha256:076e7bf82319619f28dd0aa9c02ab34b055ad37100395b4f49a2ffba04fb5e11

ENV DEBIAN_FRONTEND=noninteractive \
    CARGO_HOME=/usr/local/cargo \
    RUSTUP_HOME=/usr/local/rustup \
    PATH=/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
    LIBCLANG_PATH=/usr/lib/llvm-14/lib \
    RUSTUP_USE_CURL=1

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
      autoconf automake build-essential ca-certificates clang-14 curl file git \
      libasound2-dev libclang-14-dev libvulkan1 libx11-dev libxcursor-dev libxi-dev \
      libxrandr-dev libxkbcommon-dev libwayland-dev nasm patchelf pax-utils \
      perl pkg-config python3 tar xz-utils yasm zlib1g-dev && \
    rm -rf /var/lib/apt/lists/*

# Install an immutable Rust toolchain with a checksum-pinned rustup installer.
RUN curl --fail --silent --show-error --location --retry 5 --retry-all-errors --connect-timeout 20 --max-time 600 \
      https://static.rust-lang.org/rustup/archive/1.29.3/x86_64-unknown-linux-gnu/rustup-init \
      --output /tmp/rustup-init && \
    echo 'b1c8cb14e30889e7e2999e6b69e62d9e9b9ffc0f4df8180564d306d9680e6c55  /tmp/rustup-init' | sha256sum --check --status && \
    chmod +x /tmp/rustup-init && \
    /tmp/rustup-init -y --no-modify-path --profile minimal --default-toolchain none && \
    for attempt in 1 2 3 4 5; do \
      rustup toolchain install 1.95.0 --profile minimal --component rustfmt && break; \
      test "$attempt" != 5 || exit 1; \
    done && \
    rustup default 1.95.0 && \
    rm -f /tmp/rustup-init && \
    rustc +1.95.0 --version | grep -q '^rustc 1.95.0 '

# winit loads these desktop client libraries with dlopen; they must be explicit
# bundle roots because an ELF DT_NEEDED walk cannot discover them.
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
      libx11-xcb1 libxinerama1 libxss1 libxkbcommon-x11-0 && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /workspace
CMD ["bash"]
