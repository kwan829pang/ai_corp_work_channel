use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientFrame {
    Auth {
        access_key: String,
        nickname: String,
    },
    Join {
        token: String,
        channel: String,
    },
    Message {
        token: String,
        channel: String,
        content: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerFrame {
    AuthOk {
        token: String,
        nickname: String,
    },
    Joined {
        channel: String,
    },
    Message {
        channel: String,
        content: String,
        name: String,
        submit_at: i64,
    },
    /// Private delivery for a `#mention` target.
    Dm {
        from: String,
        to: String,
        channel: String,
        content: String,
        submit_at: i64,
    },
    /// Ack to sender after mention DM routing.
    MentionResult {
        delivered: Vec<String>,
        offline: Vec<String>,
    },
    Error {
        code: String,
        message: String,
    },
}

impl ServerFrame {
    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Error {
            code: code.into(),
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_frame_round_trip() {
        let raw = r#"{"type":"auth","access_key":"k","nickname":"alice"}"#;
        let frame: ClientFrame = serde_json::from_str(raw).expect("parse");
        match frame {
            ClientFrame::Auth {
                access_key,
                nickname,
            } => {
                assert_eq!(access_key, "k");
                assert_eq!(nickname, "alice");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn message_server_frame_shape() {
        let frame = ServerFrame::Message {
            channel: "Group A".into(),
            content: "hi #user_b".into(),
            name: "alice".into(),
            submit_at: 1730000000,
        };
        let json = serde_json::to_string(&frame).expect("serialize");
        let back: ServerFrame = serde_json::from_str(&json).expect("parse");
        assert_eq!(frame, back);
        assert!(json.contains("\"channel\":\"Group A\""));
        assert!(json.contains("\"submit_at\":1730000000"));
    }

    #[test]
    fn dm_frame_shape() {
        let frame = ServerFrame::Dm {
            from: "alice".into(),
            to: "user_b".into(),
            channel: "Group A".into(),
            content: "hi #user_b".into(),
            submit_at: 1730000000,
        };
        let json = serde_json::to_string(&frame).expect("serialize");
        assert!(json.contains("\"type\":\"dm\""));
        let back: ServerFrame = serde_json::from_str(&json).expect("parse");
        assert_eq!(frame, back);
    }
}
