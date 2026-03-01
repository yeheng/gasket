//! Message bus for inter-component communication

use tokio::sync::mpsc::{channel, Receiver, Sender};
use tracing::instrument;

use super::events::InboundMessage;

/// Message bus for routing messages between channels and agent.
///
/// The bus owns only the sender halves (cloneable). Receivers are returned
/// separately from `new()` and should be moved directly to their consumers
/// — no Mutex, no Option, no Arc needed for the receive side.
#[derive(Clone)]
pub struct MessageBus {
    inbound_tx: Sender<InboundMessage>,
}

impl MessageBus {
    /// Create a new message bus, returning the bus (senders only) plus the receiver.
    ///
    /// The caller must move `Receiver` to its single consumer at
    /// initialization time. This avoids wrapping receivers in `Arc<Mutex<Option<…>>>`.
    pub fn new(buffer_size: usize) -> (Self, Receiver<InboundMessage>) {
        let (inbound_tx, inbound_rx) = channel(buffer_size);

        (Self { inbound_tx }, inbound_rx)
    }

    /// Publish an inbound message
    #[instrument(name = "bus.publish_inbound", skip_all)]
    pub async fn publish_inbound(&self, msg: InboundMessage) {
        if let Err(e) = self.inbound_tx.send(msg).await {
            tracing::error!("Failed to publish inbound message: {}", e);
        }
    }

    /// Get a cloneable sender for inbound messages
    pub fn inbound_sender(&self) -> Sender<InboundMessage> {
        self.inbound_tx.clone()
    }
}

impl Default for MessageBus {
    fn default() -> Self {
        let (bus, _) = Self::new(100);
        bus
    }
}

/// Convenience type alias for the tuple returned by `MessageBus::new()`.
pub type MessageBusComponents = (MessageBus, Receiver<InboundMessage>);
