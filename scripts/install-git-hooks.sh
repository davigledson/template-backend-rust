#!/usr/bin/env bash
# Instala hooks locais para pegar erro antes do commit/push.
# Rode uma vez por clone: just hooks

set -euo pipefail

root=$(git rev-parse --show-toplevel)
hooks="$root/.git/hooks"
mkdir -p "$hooks"

cat > "$hooks/pre-commit" <<'HOOK'
#!/usr/bin/env bash
set -euo pipefail

cargo fmt --all -- --check

if cargo clippy --version >/dev/null 2>&1; then
    cargo clippy --all-targets --all-features -- -D warnings
else
    echo "clippy ausente; pulando lint"
fi

if cargo nextest --version >/dev/null 2>&1; then
    cargo nextest run --lib
else
    cargo test --lib
fi
HOOK

cat > "$hooks/pre-push" <<'HOOK'
#!/usr/bin/env bash
set -euo pipefail

if ! docker info >/dev/null 2>&1; then
    echo "Docker indisponível; rodando só os testes unitários"
    if cargo nextest --version >/dev/null 2>&1; then
        cargo nextest run --lib
    else
        cargo test --lib
    fi
    exit 0
fi

if cargo nextest --version >/dev/null 2>&1; then
    cargo nextest run
else
    cargo test
fi
HOOK

chmod +x "$hooks/pre-commit" "$hooks/pre-push"
echo "hooks instalados em $hooks"
