//! Hash de valores opacos (tokens e identificadores) para armazenamento.
//!
//! SHA-256 é suficiente aqui: o valor original tem alta entropia (UUIDs ou
//! códigos aleatórios), então não há ataque de dicionário a temer.

use sha2::{Digest, Sha256};

/// SHA-256 em hexadecimal minúsculo.
pub fn sha256_hex(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    let mut out = String::with_capacity(digest.len() * 2);

    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }

    out
}

/// Impressão digital estável do e-mail, para amarrar códigos ao endereço.
pub fn email_fingerprint(email: &str) -> String {
    sha256_hex(&email.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_e_estavel_e_nao_revela_o_valor() {
        let hash = sha256_hex("segredo");
        assert_eq!(hash, sha256_hex("segredo"));
        assert_eq!(hash.len(), 64);
        assert!(!hash.contains("segredo"));
    }

    #[test]
    fn fingerprint_ignora_caixa() {
        assert_eq!(
            email_fingerprint("Ana@Teste.Local"),
            email_fingerprint("ana@teste.local")
        );
        assert_ne!(
            email_fingerprint("ana@teste.local"),
            email_fingerprint("bia@teste.local")
        );
    }
}
