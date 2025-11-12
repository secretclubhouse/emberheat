# Scripts Directory

This directory contains helper scripts for development and deployment.

## Available Scripts

### `pre-commit`

Git pre-commit hook that automatically runs code quality checks before allowing a commit.

**What it checks:**
- Code formatting (`cargo fmt`)
- Linting (`cargo clippy`)
- Tests (`cargo test`)

**Installation:**

```bash
make install-hooks
```

**Manual installation:**

```bash
cp scripts/pre-commit .git/hooks/pre-commit
chmod +x .git/hooks/pre-commit
```

**Bypass (emergency only):**

```bash
git commit --no-verify
```

## Adding New Scripts

When adding new scripts to this directory:

1. Make them executable: `chmod +x scripts/your-script`
2. Document them in this README
3. Add corresponding Makefile targets if applicable
4. Use shell best practices:
   - Use `set -e` to exit on errors
   - Add comments explaining what the script does
   - Include usage instructions in comments
