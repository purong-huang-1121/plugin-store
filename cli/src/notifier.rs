//! Shared Telegram notification support for all strategy modules.

use reqwest::Client;
use serde_json::json;

/// Severity level for notifications.
#[derive(Debug, Clone, Copy)]
pub enum NotifyLevel {
    Info,
    Success,
    Warning,
    Error,
}

impl NotifyLevel {
    /// Return the emoji representing this level.
    pub fn emoji(self) -> &'static str {
        match self {
            NotifyLevel::Info => "ℹ️",
            NotifyLevel::Success => "✅",
            NotifyLevel::Warning => "⚠️",
            NotifyLevel::Error => "🚨",
        }
    }

    fn label(self) -> &'static str {
        match self {
            NotifyLevel::Info => "INFO",
            NotifyLevel::Success => "SUCCESS",
            NotifyLevel::Warning => "WARNING",
            NotifyLevel::Error => "ERROR",
        }
    }
}

/// Sends notifications to stderr and optionally to Telegram.
pub struct Notifier {
    telegram_token: Option<String>,
    telegram_chat_id: Option<String>,
    http: Client,
    /// Display name used in Telegram messages (e.g. "Grid Bot", "Auto-Rebalancer").
    bot_name: String,
}

impl Notifier {
    /// Create a new notifier with explicit token, chat_id, and bot name.
    pub fn new(
        telegram_token: Option<String>,
        telegram_chat_id: Option<String>,
        bot_name: &str,
    ) -> Self {
        Self {
            telegram_token,
            telegram_chat_id,
            http: Client::new(),
            bot_name: bot_name.to_string(),
        }
    }

    /// Create a notifier from `TELEGRAM_BOT_TOKEN` and `TELEGRAM_CHAT_ID` env vars.
    pub fn from_env(bot_name: &str) -> Self {
        let token = std::env::var("TELEGRAM_BOT_TOKEN").ok();
        let chat_id = std::env::var("TELEGRAM_CHAT_ID").ok();
        Self::new(token, chat_id, bot_name)
    }

    /// Returns true if Telegram credentials are configured.
    pub fn is_configured(&self) -> bool {
        self.telegram_token.is_some() && self.telegram_chat_id.is_some()
    }

    /// Log to stderr and optionally send a Telegram message.
    pub async fn notify(&self, level: NotifyLevel, message: &str) {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
        eprintln!("[{now}] [{label}] {message}", label = level.label());

        if let (Some(token), Some(chat_id)) = (&self.telegram_token, &self.telegram_chat_id) {
            let text = format!("{} *{}*\n\n{}", level.emoji(), self.bot_name, message);
            let url = format!("https://api.telegram.org/bot{token}/sendMessage");
            let body = json!({
                "chat_id": chat_id,
                "text": text,
                "parse_mode": "Markdown",
            });

            if let Err(e) = self.http.post(&url).json(&body).send().await {
                eprintln!("[{now}] [WARNING] Failed to send Telegram notification: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn notifier_without_config_is_noop() {
        let notifier = Notifier::new(None, None, "Test Bot");
        // Should not panic even without Telegram config.
        notifier.notify(NotifyLevel::Info, "test message").await;
    }

    #[test]
    fn notify_level_emoji() {
        assert_eq!(NotifyLevel::Info.emoji(), "ℹ️");
        assert_eq!(NotifyLevel::Success.emoji(), "✅");
        assert_eq!(NotifyLevel::Warning.emoji(), "⚠️");
        assert_eq!(NotifyLevel::Error.emoji(), "🚨");
    }

    #[test]
    fn format_message_contains_bot_name() {
        let notifier = Notifier::new(None, None, "Grid Bot");
        assert_eq!(notifier.bot_name, "Grid Bot");
    }

    #[test]
    fn is_configured_false_without_creds() {
        let notifier = Notifier::new(None, None, "Test");
        assert!(!notifier.is_configured());
    }

    #[test]
    fn is_configured_true_with_creds() {
        let notifier = Notifier::new(Some("token".into()), Some("chat".into()), "Test");
        assert!(notifier.is_configured());
    }
}
