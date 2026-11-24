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
}
