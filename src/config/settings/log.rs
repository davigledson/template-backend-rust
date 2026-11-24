//! Formato dos logs: nível, saída e ANSI.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct LogSettings {
    pub level: String,
    pub format: String,
    pub ansi: bool,
}

impl LogSettings {
    pub(super) fn collect_errors(&self, problems: &mut Vec<String>) {
        if !matches!(self.format.as_str(), "json" | "pretty" | "compact") {
            problems.push("log.format deve ser json, pretty ou compact".to_owned());
        }
    }
}

impl Default for LogSettings {
    fn default() -> Self {
        Self {
            level: "info".to_owned(),
            format: "json".to_owned(),
            ansi: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn errors(settings: &LogSettings) -> String {
        let mut problems = Vec::new();
        settings.collect_errors(&mut problems);
        problems.join("; ")
    }

    #[test]
    fn rejeita_formato_desconhecido() {
        assert!(errors(&LogSettings::default()).is_empty());

        let settings = LogSettings {
            format: "yaml".to_owned(),
            ..Default::default()
        };
        assert!(errors(&settings).contains("log.format"));
    }
}
