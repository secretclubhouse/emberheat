#!/bin/bash
set -e

echo "Running security checks..."

# Check .env is not staged
if git diff --cached --name-only | grep -E '^\.env$|^\.env\.' | grep -v '\.example$'; then
  echo "ERROR: .env file is staged for commit!"
  exit 1
fi

# Check for config.json (contains sensitive data)
if git diff --cached --name-only | grep -q 'config/config.json'; then
  echo "ERROR: config/config.json contains sensitive data and should not be committed!"
  exit 1
fi

# Check for common secret patterns in staged files
STAGED_FILES=$(git diff --cached --name-only --diff-filter=ACM)
if [ -n "$STAGED_FILES" ]; then
  # Check for hardcoded secrets
  if echo "$STAGED_FILES" | xargs grep -l -E '(password|secret|api_key|apikey|token)\s*[:=]\s*["\047][^"\047]{8,}["\047]' 2>/dev/null; then
    echo "WARNING: Possible secrets found in staged files - please verify"
  fi

  # Check for Redis URLs with passwords
  if echo "$STAGED_FILES" | xargs grep -l -E 'redis://[^@]*:[^@]+@' 2>/dev/null; then
    echo "WARNING: Redis URL with password found - consider using environment variable"
  fi
fi

# Cargo audit (check for security vulnerabilities in dependencies)
if command -v cargo-audit &> /dev/null; then
  echo "Checking Rust dependencies for vulnerabilities..."
  cargo audit 2>/dev/null || echo "Warning: cargo audit found issues"
else
  echo "⚠ cargo-audit not installed. Run: cargo install cargo-audit"
fi

# Check for outdated dependencies with known issues
echo "Checking for outdated dependencies..."
cargo update --dry-run 2>/dev/null || echo "Dependencies are up to date"

echo ""
echo "Security checks complete!"
