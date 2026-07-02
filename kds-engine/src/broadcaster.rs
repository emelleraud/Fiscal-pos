use std::sync::Arc;

use tokio::sync::broadcast;

use crate::types::event::KdsEvent;

/// Broadcaster SSE partagé entre tous les handlers Axum.
/// Capacité 256 — largement suffisant pour un restaurant (< 20 commandes simultanées).
#[derive(Clone, Debug)]
pub struct KdsBroadcaster {
    sender: Arc<broadcast::Sender<KdsEvent>>,
}

impl KdsBroadcaster {
    #[must_use]
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(256);
        Self {
            sender: Arc::new(sender),
        }
    }

    /// Envoie un événement à tous les abonnés actifs.
    /// Ignore silencieusement si aucun abonné (normal au démarrage).
    pub fn send(&self, event: KdsEvent) {
        let _ = self.sender.send(event);
    }

    /// Crée un nouveau Receiver pour un handler SSE.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<KdsEvent> {
        self.sender.subscribe()
    }
}

impl Default for KdsBroadcaster {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::event::{KdsAckPayload, KdsEvent};

    #[tokio::test]
    async fn broadcast_reaches_subscriber() {
        let broadcaster = KdsBroadcaster::new();
        let mut rx = broadcaster.subscribe();

        let event = KdsEvent::OrderAcked(KdsAckPayload {
            order_id: "ord-1".to_string(),
            station_id: "grill".to_string(),
            line_id: None,
        });

        broadcaster.send(event.clone());
        let received = rx.try_recv().expect("event should be received");
        assert!(matches!(received, KdsEvent::OrderAcked(_)));
    }

    #[tokio::test]
    async fn no_subscriber_does_not_panic() {
        let broadcaster = KdsBroadcaster::new();
        broadcaster.send(KdsEvent::OrderAcked(KdsAckPayload {
            order_id: "ord-2".to_string(),
            station_id: "grill".to_string(),
            line_id: None,
        }));
        // pas de panique = succès
    }
}
