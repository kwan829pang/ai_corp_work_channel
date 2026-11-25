use crate::mention::extract_mentions;
use crate::protocol::ServerFrame;
use crate::server::state::SharedState;
use chrono::Utc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishOutcome {
    pub submit_at: i64,
    pub recipients: usize,
    pub mentions_delivered: Vec<String>,
    pub mentions_offline: Vec<String>,
}

/// One-shot channel publish: broadcast `message` and route `#mention` DMs.
pub async fn publish_message(
    state: &SharedState,
    channel: &str,
    name: &str,
    content: &str,
) -> PublishOutcome {
    let submit_at = Utc::now().timestamp();
    let frame = ServerFrame::Message {
        channel: channel.to_string(),
        content: content.to_string(),
        name: name.to_string(),
        submit_at,
    };

    let recipients = state.broadcast(channel, frame, None).await;
    let (mentions_delivered, mentions_offline) =
        route_mention_dms(state, channel, name, content, submit_at).await;

    PublishOutcome {
        submit_at,
        recipients,
        mentions_delivered,
        mentions_offline,
    }
}

async fn route_mention_dms(
    state: &SharedState,
    channel: &str,
    name: &str,
    content: &str,
    submit_at: i64,
) -> (Vec<String>, Vec<String>) {
    let mentions = extract_mentions(content);
    if mentions.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let sender_key = name.to_ascii_lowercase();
    let mut delivered = Vec::new();
    let mut offline = Vec::new();

    for mention in mentions {
        if mention.to_ascii_lowercase() == sender_key {
            continue;
        }

        let dm = ServerFrame::Dm {
            from: name.to_string(),
            to: mention.clone(),
            channel: channel.to_string(),
            content: content.to_string(),
            submit_at,
        };

        if state.send_to_nickname(&mention, dm).await {
            delivered.push(mention);
        } else {
            offline.push(mention);
        }
    }

    (delivered, offline)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ServerFrame;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn publishes_to_channel_members() {
        let state = SharedState::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        state.try_reserve_nickname(1, "alice").await.expect("nick");
        state.register_outbound(1, tx.clone()).await;
        state.join_channel(1, "Group A", tx).await.expect("join");

        let outcome = publish_message(&state, "Group A", "system", "hello").await;
        assert_eq!(outcome.recipients, 1);

        let frame = rx.recv().await.expect("frame");
        match frame {
            ServerFrame::Message {
                channel,
                content,
                name,
                ..
            } => {
                assert_eq!(channel, "Group A");
                assert_eq!(content, "hello");
                assert_eq!(name, "system");
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
