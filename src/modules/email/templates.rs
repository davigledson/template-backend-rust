//! Textos dos e-mails transacionais.
//!
//! Mantidos em código para o template não depender de template engine. Nunca
//! inclua senha em e-mail.

use crate::core::mail::EmailMessage;

pub fn welcome(name: &str, to: &str) -> EmailMessage {
    EmailMessage {
        to: to.to_owned(),
        subject: "Sua conta foi criada".to_owned(),
        body: format!(
            "Olá, {name}.\n\n\
             Sua conta foi criada por um administrador.\n\
             Use o e-mail {to} e a senha definida no cadastro para entrar.\n\n\
             Se você não esperava este e-mail, ignore-o.\n"
        ),
    }
}

pub fn password_changed(name: &str, to: &str) -> EmailMessage {
    EmailMessage {
        to: to.to_owned(),
        subject: "Sua senha foi alterada".to_owned(),
        body: format!(
            "Olá, {name}.\n\n\
             A senha da sua conta ({to}) foi redefinida por um administrador.\n\
             Se não foi você quem pediu, entre em contato com o suporte.\n"
        ),
    }
}

pub fn password_changed_by_user(name: &str, to: &str) -> EmailMessage {
    EmailMessage {
        to: to.to_owned(),
        subject: "Sua senha foi alterada".to_owned(),
        body: format!(
            "Olá, {name}.\n\n\
             A senha da sua conta ({to}) foi alterada por você.\n\
             Todas as sessões foram encerradas; entre novamente com a nova senha.\n\n\
             Se não foi você, redefina a senha imediatamente e avise o suporte.\n"
        ),
    }
}

pub fn password_reset(name: &str, to: &str, token: &str) -> EmailMessage {
    EmailMessage {
        to: to.to_owned(),
        subject: "Redefinição de senha".to_owned(),
        body: format!(
            "Olá, {name}.\n\n\
             Recebemos um pedido para redefinir a senha da sua conta ({to}).\n\
             Use o código abaixo para definir a nova senha.\n\n\
             {token}\n\n\
             O código vale por 30 minutos e só pode ser usado uma vez.\n\
             Se não foi você, ignore este e-mail: a senha atual continua valendo.\n"
        ),
    }
}

pub fn verify_email(name: &str, to: &str, token: &str) -> EmailMessage {
    EmailMessage {
        to: to.to_owned(),
        subject: "Confirme seu e-mail".to_owned(),
        body: format!(
            "Olá, {name}.\n\n\
             Confirme o e-mail {to} usando o código abaixo:\n\n\
             {token}\n\n\
             O código vale por 7 dias e só pode ser usado uma vez.\n"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boas_vindas_nao_contem_senha() {
        let message = welcome("Ana", "ana@teste.local");
        assert!(message.body.contains("Ana"));
        assert!(!message.body.to_lowercase().contains("senha:"));
    }

    #[test]
    fn aviso_de_senha_tem_assunto_proprio() {
        let message = password_changed("Ana", "ana@teste.local");
        assert_eq!(message.subject, "Sua senha foi alterada");
    }

    #[test]
    fn recuperacao_leva_o_codigo_sem_senha() {
        let message = password_reset("Ana", "ana@teste.local", "abc123");
        assert!(message.body.contains("abc123"));
        assert!(message.body.contains("30 minutos"));
        assert!(!message.body.to_lowercase().contains("senha:"));
    }

    #[test]
    fn verificacao_leva_o_codigo() {
        let message = verify_email("Ana", "ana@teste.local", "def456");
        assert_eq!(message.subject, "Confirme seu e-mail");
        assert!(message.body.contains("def456"));
    }

    #[test]
    fn aviso_de_troca_propria_nao_cita_admin() {
        let message = password_changed_by_user("Ana", "ana@teste.local");
        assert!(message.body.contains("por você"));
        assert!(!message.body.contains("administrador"));
    }
}
