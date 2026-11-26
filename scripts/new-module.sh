#!/usr/bin/env bash
# Gera o esqueleto de um módulo novo em src/modules/<nome>.
#
# O módulo nasce com uma rota autenticada que responde 501, registrada no router
# e no OpenAPI. Tabela, DTOs e permissões ficam para o desenvolvedor.
set -euo pipefail

usage() {
    cat <<'USAGE'
Uso: just module <nome> [--dry-run]

  <nome>       notes, notas_fiscais ou notas-fiscais (vira notas_fiscais)
  --dry-run    mostra o que seria criado, sem alterar nada
USAGE
}

die() {
    echo "module: $*" >&2
    exit 1
}

sed_inplace() {
    if sed --version >/dev/null 2>&1; then
        sed -i "$@"
    else
        sed -i '' "$@"
    fi
}

dry_run=false
name=""

while [ "$#" -gt 0 ]; do
    case "$1" in
        --dry-run) dry_run=true ;;
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

module="$(
    printf '%s' "$name" \
        | tr '[:upper:]' '[:lower:]' \
        | tr ' -' '__' \
        | sed -E 's/_+/_/g; s/^_//; s/_$//'
)"
[ -n "$module" ] || die "nome vazio"
printf '%s' "$module" | grep -Eq '^[a-z][a-z0-9]*(_[a-z0-9]+)*$' \
    || die "nome inválido: use letras minúsculas, números e underscore (ex.: notas_fiscais)"
[ "${#module}" -le 40 ] || die "nome longo demais (máximo 40 caracteres)"

case "$module" in
    self | super | crate | mod | use | fn | struct | enum | trait | impl | type | const | static | \
        let | match | if | else | for | while | loop | return | async | await | move | ref | pub | in | \
        where | dyn | box | break | continue | extern | unsafe | virtual | yield | do | final | macro | \
        override | priv | try | typeof | unsized | become | abstract)
        die "nome reservado: $module"
        ;;
esac

dir="src/modules/$module"
[ -e "$dir" ] && die "módulo já existe: $dir"
grep -q "^pub mod $module;" src/modules/mod.rs && die "módulo já registrado em src/modules/mod.rs"

route="${module//_/-}"

echo "Módulo:   $module"
echo "Rota:     GET /api/v1/$route"
echo "Arquivos:"
printf '  %s\n' "$dir/mod.rs" "$dir/controller.rs" "$dir/service.rs" \
    "$dir/repository.rs" "$dir/dto.rs"
echo "Registro: src/modules/mod.rs (pub mod + merge do router)"

if [ "$dry_run" = true ]; then
    echo
    echo "--dry-run: nada foi alterado."
    exit 0
fi

mkdir -p "$dir"

cat > "$dir/mod.rs" <<'EOF'
//! Módulo __MODULE__.
//!
//! Esqueleto gerado por `just module __MODULE__`. A rota atual responde 501;
//! troque-a pela regra real e use repository.rs/dto.rs quando houver tabela e
//! contrato. A migration começa com `just migration create___MODULE__`.

pub mod controller;
pub mod dto;
pub mod repository;
pub mod service;

pub use controller::router;
EOF

cat > "$dir/controller.rs" <<'EOF'
//! Rotas do módulo __MODULE__.

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    core::{auth::extractor::AuthUser, error::AppError, openapi::Problem, state::AppState},
    modules::__MODULE__::service,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(status))
}

#[utoipa::path(
    get,
    path = "/__ROUTE__",
    tag = "__MODULE__",
    summary = "Status do módulo __MODULE__",
    description = "Placeholder criado por `just module`. Substitua quando a regra existir.",
    security(("bearer" = [])),
    responses(
        (status = 401, description = "Não autenticado", body = Problem),
        (status = 501, description = "Módulo ainda não implementado", body = Problem),
    )
)]
async fn status(_auth: AuthUser) -> Result<(), AppError> {
    Err(service::not_implemented())
}
EOF

cat > "$dir/service.rs" <<'EOF'
//! Regras do módulo __MODULE__.

use crate::core::error::AppError;

/// Resposta do esqueleto até a regra real existir.
pub fn not_implemented() -> AppError {
    AppError::NotImplemented("módulo __MODULE__ ainda não implementado".to_owned())
}
EOF

cat > "$dir/repository.rs" <<'EOF'
//! Acesso à tabela do módulo __MODULE__.
//!
//! Crie as consultas aqui quando a migration existir; mantenha o SeaORM neste
//! módulo e não acesse a tabela de fora.
EOF

cat > "$dir/dto.rs" <<'EOF'
//! Contratos JSON do módulo __MODULE__.
//!
//! Adicione os tipos de entrada/saída quando definir a API.
EOF

sed_inplace "s|__MODULE__|${module}|g; s|__ROUTE__|${route}|g" \
    "$dir/mod.rs" "$dir/controller.rs" "$dir/service.rs" \
    "$dir/repository.rs" "$dir/dto.rs"

# Registro em ordem alfabética no bloco `pub mod ...;`.
awk -v module="$module" '
    BEGIN { inserted = 0 }
    !inserted && /^pub mod / {
        name = $3
        sub(/;$/, "", name)
        if (name > module) {
            print "pub mod " module ";"
            inserted = 1
        }
    }
    { print }
    END {
        if (!inserted) {
            print "pub mod " module ";"
        }
    }
' src/modules/mod.rs > src/modules/mod.rs.tmp
mv src/modules/mod.rs.tmp src/modules/mod.rs

# Junta o router do módulo na cadeia existente.
awk -v module="$module" '
    { lines[NR] = $0 }
    /\.merge\([a-z_]+::router\(\)\)/ { last = NR }
    END {
        for (i = 1; i <= NR; i++) {
            print lines[i]
            if (i == last) {
                print "        .merge(" module "::router())"
            }
        }
    }
' src/modules/mod.rs > src/modules/mod.rs.tmp
mv src/modules/mod.rs.tmp src/modules/mod.rs

grep -q "^pub mod $module;" src/modules/mod.rs || die "falha ao registrar pub mod $module"
grep -q "\.merge($module::router())" src/modules/mod.rs || die "falha ao registrar o router"

echo
echo "Atualizando o snapshot do OpenAPI e formatando..."
if cargo nextest --version >/dev/null 2>&1; then
    INSTA_UPDATE=always cargo nextest run --lib routes::openapi >/dev/null
else
    INSTA_UPDATE=always cargo test --lib routes::openapi >/dev/null
fi
cargo fmt --all
cargo check --all-targets --all-features --quiet
if cargo nextest --version >/dev/null 2>&1; then
    cargo nextest run --lib
else
    cargo test --lib
fi

echo
echo "Pronto: $dir"
cat <<EOF

Próximos passos:
  - defina a tabela: just migration create_${module} e preencha up/down;
  - aplique e regenere a entidade: just migrate && just entities;
  - troque a rota 501 pela regra real (service/repository/dto);
  - se a rota for sensível, crie a permissão em core/auth/permission.rs e chame
    require_session no handler;
  - revise o snapshot e o diff, e commite.
EOF
