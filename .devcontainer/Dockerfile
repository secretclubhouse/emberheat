# Use Debian-based Rust image for better compatibility with GPIO libraries
FROM rust:1.90-slim-trixie

# Avoid warnings by switching to noninteractive
ENV DEBIAN_FRONTEND=noninteractive

# Install system dependencies
RUN apt-get update && apt-get install -y \
    # Build essentials
    build-essential \
    pkg-config \
    # C++ compiler and tools for Matter wrapper
    gcc \
    g++ \
    clang \
    cmake \
    ninja-build \
    unzip \
    # SSL/TLS support
    libssl-dev \
    # Matter protocol dependencies
    libdbus-1-dev \
    libglib2.0-dev \
    libavahi-client-dev \
    libreadline-dev \
    libgirepository1.0-dev \
    libcairo2-dev \
    # Python (required for Matter SDK build scripts)
    python3 \
    python3-pip \
    python3-venv \
    python3-dev \
    # Java runtime (required for Matter ZAP tool)
    default-jre \
    # Debugging tools
    gdb \
    lldb \
    # Networking tools
    curl \
    wget \
    netcat-openbsd \
    # Version control
    git \
    git-lfs \
    # Process monitoring
    htop \
    procps \
    # Text editors (fallback)
    vim \
    nano \
    # Redis client
    redis-tools \
    # Additional tools
    jq \
    tree \
    && rm -rf /var/lib/apt/lists/*

# Install Rust components
RUN rustup component add rustfmt clippy rust-src rust-analyzer

# Create a non-root user
ARG USERNAME=vscode
ARG USER_UID=1000
ARG USER_GID=$USER_UID

RUN groupadd --gid $USER_GID $USERNAME \
    && useradd --uid $USER_UID --gid $USER_GID -m $USERNAME \
    # Add sudo support
    && apt-get update \
    && apt-get install -y sudo \
    && echo $USERNAME ALL=\(root\) NOPASSWD:ALL > /etc/sudoers.d/$USERNAME \
    && chmod 0440 /etc/sudoers.d/$USERNAME \
    && rm -rf /var/lib/apt/lists/*

# Set up cargo cache directory with proper permissions
RUN mkdir -p /usr/local/cargo/registry && chown -R $USERNAME:$USERNAME /usr/local/cargo

# Switch back to dialog for any ad-hoc use of apt-get
ENV DEBIAN_FRONTEND=dialog

# Set the default user
USER $USERNAME

# Pre-install common Rust tools as the vscode user
RUN cargo install cargo-watch cargo-expand cargo-tree

WORKDIR /workspace
