//! Codificação do valor guardado no Redis: `user_id:token_version:family_id`.

use uuid::Uuid;

use crate::modules::auth::repository::RefreshTicket;

pub(super) fn parse_ticket(value: &str) -> Option<RefreshTicket> {
    let mut parts = value.split(':');

    let user_id = Uuid::parse_str(parts.next()?).ok()?;
    let token_version = parts.next()?.parse::<i32>().ok()?;
    let family_id = Uuid::parse_str(parts.next()?).ok()?;

    Some(RefreshTicket {
        user_id,
        token_version,
        family_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticket_ida_e_volta() {
        let ticket = RefreshTicket {
            user_id: Uuid::now_v7(),
            token_version: 3,
            family_id: Uuid::now_v7(),
        };
        let value = format!(
            "{}:{}:{}",
            ticket.user_id, ticket.token_version, ticket.family_id
        );

        let parsed = parse_ticket(&value).expect("ticket");
        assert_eq!(parsed.user_id, ticket.user_id);
        assert_eq!(parsed.token_version, 3);
        assert_eq!(parsed.family_id, ticket.family_id);
    }

    #[test]
    fn formato_antigo_vira_unknown() {
        let value = format!("{}:0", Uuid::now_v7());
        assert!(parse_ticket(&value).is_none());
    }
}
