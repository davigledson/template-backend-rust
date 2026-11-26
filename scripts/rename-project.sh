#!/usr/bin/env bash
# Renomeia o projeto criado a partir do template.
#
# Troca os identificadores técnicos (crate, imports, compose, tags, OpenAPI e
# JWT) e mantém a prosa que fala "template". Exige árvore git limpa para que o
# rename seja reversível com `git reset --hard`.
set -euo pipefail

OLD_KEBAB="template-backend-rust"
OLD_SNAKE="template_backend_rust"
OLD_TITLE="Template Backend Rust"
OLD_ISSUER="template-api"
OLD_AUDIENCE="template-front"

usage() {
    cat <<'USAGE'
Uso: just rename <nome> [--dry-run] [--yes]

  <nome>       projeto-do-cliente, projeto_do_cliente ou "Projeto do Cliente"
  --dry-run    mostra o que mudaria, sem alterar nada
  --yes        não pede confirmação
USAGE
}

die() {
    echo "rename: $*" >&2
    exit 1
}

dry_run=false
assume_yes=false
name=""

while [ "$#" -gt 0 ]; do
    case "$1" in
        --dry-run) dry_run=true ;;
        --yes | -y) assume_yes=true ;;
        -h | --help)
            usage
            exit 0
            ;;
        -*)
            usage >&2
            die "opção desconhecida: $1"
            ;;
        *)
            [ -z "$name" ] || die "informe apenas um nome"
            name="$1"
            ;;
    esac
    shift
done

if [ -z "$name" ]; then
    usage >&2
    exit 2
fi

root="$(git rev-parse --show-toplevel 2>/dev/null)" || die "rode dentro do repositório git"
cd "$root"

normalized="$(
    printf '%s' "$name" \
        | tr '[:upper:]' '[:lower:]' \
        | tr ' _' '--' \
        | sed -E 's/-+/-/g; s/^-//; s/-$//'
)"
[ -n "$normalized" ] || die "nome vazio"
printf '%s' "$normalized" | grep -Eq '^[a-z][a-z0-9]*(-[a-z0-9]+)*$' \
    || die "nome inválido: use letras minúsculas, números e hífen (ex.: acme-backend)"
[ "${#normalized}" -le 48 ] || die "nome longo demais (máximo 48 caracteres)"
[ "$normalized" != "$OLD_KEBAB" ] || die "o projeto já se chama $OLD_KEBAB"

grep -q "^name = \"$OLD_KEBAB\"" Cargo.toml \
    || die "Cargo.toml não tem name = \"$OLD_KEBAB\"; o projeto já foi renomeado?"

snake="${normalized//-/_}"
title="$(
    printf '%s' "$normalized" \
        | awk -F- '{
            for (i = 1; i <= NF; i++) {
                printf "%s%s", toupper(substr($i, 1, 1)), substr($i, 2)
                if (i < NF) printf " "
            }
        }'
)"
issuer="${normalized}-api"
audience="${normalized}-front"

if [ "$dry_run" = false ] && [ -n "$(git status --porcelain)" ]; then
    die "árvore git suja; commite ou descarte as mudanças antes do rename"
fi

files=()
while IFS= read -r -d '' path; do
    files+=("$path")
done < <(
    git grep -l -z \
        -e "$OLD_KEBAB" -e "$OLD_SNAKE" -e "$OLD_TITLE" \
        -e "$OLD_ISSUER" -e "$OLD_AUDIENCE" -- . \
        ':(exclude)Cargo.lock' \
        ':(exclude)docs/rename.md' \
        ':(exclude)scripts/rename-project.sh' || true
)

[ "${#files[@]}" -gt 0 ] || die "nenhum arquivo com o nome antigo; nada a fazer"

snapshots=()
while IFS= read -r path; do
    [ -n "$path" ] && snapshots+=("$path")
done < <(git ls-files "src/**/snapshots/${OLD_SNAKE}__*.snap")

echo "Projeto:  $OLD_KEBAB -> $normalized"
echo "Rust:     $OLD_SNAKE -> $snake"
echo "Título:   $OLD_TITLE -> $title"
echo "JWT:      $OLD_ISSUER/$OLD_AUDIENCE -> $issuer/$audience"
echo "Arquivos: ${#files[@]}"
printf '  %s\n' "${files[@]}"
if [ "${#snapshots[@]}" -gt 0 ]; then
    echo "Snapshots:"
    printf '  %s\n' "${snapshots[@]}"
fi
if [ -f .env ]; then
    echo ".env: RUST_LOG será atualizado"
fi

if [ "$dry_run" = true ]; then
    echo
    echo "--dry-run: nada foi alterado."
    exit 0
fi

if [ "$assume_yes" = false ]; then
    [ -t 0 ] || die "sem terminal para confirmar; use --yes"
    read -r -p "Digite '$normalized' para confirmar: " answer
    [ "$answer" = "$normalized" ] || die "cancelado"
fi

sed_inplace() {
    if sed --version >/dev/null 2>&1; then
        sed -i "$@"
    else
        sed -i '' "$@"
    fi
}

replace() {
    local old="$1" new="$2"
    sed_inplace "s|${old}|${new}|g" -- "${files[@]}"
}

replace "$OLD_TITLE" "$title"
replace "$OLD_SNAKE" "$snake"
replace "$OLD_KEBAB" "$normalized"
replace "$OLD_ISSUER" "$issuer"
replace "$OLD_AUDIENCE" "$audience"

for old_snap in "${snapshots[@]}"; do
    dir="$(dirname "$old_snap")"
    base="$(basename "$old_snap")"
    git mv "$old_snap" "$dir/${snake}${base#"$OLD_SNAKE"}"
done

if [ -f .env ]; then
    sed_inplace "s|${OLD_SNAKE}|${snake}|g" .env
fi

grep -q "^name = \"$normalized\"" Cargo.toml || die "falha ao renomear Cargo.toml"

echo
echo "Atualizando Cargo.lock e formatando..."
cargo check --all-targets --all-features --quiet
cargo fmt --all
if cargo nextest --version >/dev/null 2>&1; then
    cargo nextest run --lib
else
    cargo test --lib
fi

echo
echo "Pronto: $normalized"
git diff --stat
cat <<EOF

Checklist:
  - revise a description em Cargo.toml e a prosa "template" nos docs;
  - gere um novo APP__JWT__SECRET (issuer/audience mudaram e invalidam tokens);
  - configure remote, CI e os segredos do deploy;
  - rode \`just up && just migrate\` e crie o primeiro admin;
  - confira o diff e commite.
EOF
