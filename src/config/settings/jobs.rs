//! Worker de cron: liga/desliga e fuso dos agendamentos.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct JobSettings {
    pub enabled: bool,
    pub timezone: String,
}

impl JobSettings {
    pub(super) fn collect_errors(&self, problems: &mut Vec<String>) {
        if self.timezone.parse::<chrono_tz::Tz>().is_err() {
            problems.push(format!(
                "jobs.timezone não é um fuso IANA válido: {}",
                self.timezone
            ));
        }
    }
}

impl Default for JobSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            timezone: "America/Sao_Paulo".to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn errors(settings: &JobSettings) -> String {
        let mut problems = Vec::new();
        settings.collect_errors(&mut problems);
        problems.join("; ")
    }

    #[test]
    fn rejeita_timezone_invalido() {
        assert!(errors(&JobSettings::default()).is_empty());

        let settings = JobSettings {
            timezone: "Marte/Olympus".to_owned(),
            ..Default::default()
        };
        assert!(errors(&settings).contains("jobs.timezone"));
    }
}
