use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::error::ProviderError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// Services the host (the app) provides to a provider.
pub trait Host: Send + Sync {
    fn log(&self, level: LogLevel, message: &str);
    fn data_dir(&self) -> PathBuf;
    fn setting(&self, key: &str) -> Option<String>;
    fn secret(&self, key: &str) -> Option<String>;
    /// `None` deletes the secret.
    fn set_secret(&self, key: &str, value: Option<&str>) -> Result<(), ProviderError>;
    fn open_url(&self, url: &str) -> Result<(), ProviderError>;
    fn cancelled(&self) -> bool;
}

pub struct Call<'a> {
    pub deadline: Option<Instant>,
    pub host: &'a dyn Host,
}

impl Call<'_> {
    /// Time left until the deadline, zero once it has passed; `None` when the call has no deadline.
    pub fn remaining(&self) -> Option<Duration> {
        self.deadline
            .map(|deadline| deadline.saturating_duration_since(Instant::now()))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct RecordingHost {
        logs: Mutex<Vec<(LogLevel, String)>>,
        secrets: Mutex<Vec<(String, Option<String>)>>,
    }

    impl Host for RecordingHost {
        fn log(&self, level: LogLevel, message: &str) {
            self.logs.lock().unwrap().push((level, message.to_string()));
        }

        fn data_dir(&self) -> PathBuf {
            PathBuf::from("/tmp/clouddeck")
        }

        fn setting(&self, key: &str) -> Option<String> {
            (key == "region").then(|| "us-east-1".to_string())
        }

        fn secret(&self, _key: &str) -> Option<String> {
            None
        }

        fn set_secret(&self, key: &str, value: Option<&str>) -> Result<(), ProviderError> {
            self.secrets
                .lock()
                .unwrap()
                .push((key.to_string(), value.map(str::to_string)));
            Ok(())
        }

        fn open_url(&self, _url: &str) -> Result<(), ProviderError> {
            Ok(())
        }

        fn cancelled(&self) -> bool {
            false
        }
    }

    #[test]
    fn remaining_counts_down_to_the_deadline_and_saturates() {
        let host = RecordingHost::default();
        let open = Call {
            deadline: Some(Instant::now() + Duration::from_secs(60)),
            host: &host,
        };
        let left = open.remaining().expect("deadline set");
        assert!(left > Duration::from_secs(55) && left <= Duration::from_secs(60));

        let passed = Call {
            deadline: Some(Instant::now() - Duration::from_secs(1)),
            host: &host,
        };
        assert_eq!(passed.remaining(), Some(Duration::ZERO));

        let unbounded = Call {
            deadline: None,
            host: &host,
        };
        assert_eq!(unbounded.remaining(), None);
    }

    #[test]
    fn call_exposes_host_and_deadline() {
        let host = RecordingHost::default();
        let deadline = Instant::now();
        let call = Call {
            deadline: Some(deadline),
            host: &host,
        };

        call.host.log(LogLevel::Info, "listing");
        assert_eq!(call.host.setting("region").as_deref(), Some("us-east-1"));
        assert_eq!(call.host.setting("missing"), None);
        assert_eq!(call.deadline, Some(deadline));
        assert!(!call.host.cancelled());
        assert_eq!(call.host.data_dir(), PathBuf::from("/tmp/clouddeck"));
        call.host.set_secret("token", Some("abc")).unwrap();
        call.host.set_secret("token", None).unwrap();

        assert_eq!(
            *host.logs.lock().unwrap(),
            vec![(LogLevel::Info, "listing".to_string())]
        );
        assert_eq!(
            *host.secrets.lock().unwrap(),
            vec![
                ("token".to_string(), Some("abc".to_string())),
                ("token".to_string(), None),
            ]
        );
    }
}
