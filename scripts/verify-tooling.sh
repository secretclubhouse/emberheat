#!/bin/bash
set -e

echo "Verifying project tooling..."

# GitHub CLI
if command -v gh &> /dev/null; then
  if gh auth status &> /dev/null; then
    echo "✓ GitHub CLI authenticated"
  else
    echo "✗ GitHub CLI not authenticated. Run: gh auth login"
    exit 1
  fi
else
  echo "⚠ GitHub CLI not installed. Run: brew install gh"
fi

# Rust toolchain
if command -v cargo &> /dev/null; then
  echo "✓ Rust toolchain installed ($(rustc --version))"
else
  echo "✗ Rust not installed. Run: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
  exit 1
fi

# cargo-tarpaulin (for test coverage)
if cargo --list | grep -q tarpaulin; then
  echo "✓ cargo-tarpaulin installed"
else
  echo "⚠ cargo-tarpaulin not installed. Run: cargo install cargo-tarpaulin"
fi

# clippy (linter)
if command -v cargo-clippy &> /dev/null; then
  echo "✓ clippy installed"
else
  echo "⚠ clippy not installed. Run: rustup component add clippy"
fi

# rustfmt (formatter)
if command -v rustfmt &> /dev/null; then
  echo "✓ rustfmt installed"
else
  echo "⚠ rustfmt not installed. Run: rustup component add rustfmt"
fi

echo ""
echo "Tooling verification complete!"
