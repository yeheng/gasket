//! Message bus for inter-component communication

use std::sync::Arc;

use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::Mutex;

use super::events::{InboundMessage, OutboundMessage};

/// Message bus for routing messages between channels and agent.
///
/// The bus owns the sender halves (cloneable) and allows the receiver halves
/// to be taken once via `take_inbound_receiver` / `take_outbound_receiver`.
/// This avoids wrapping `Receiver` in `Mutex` for ongoing consumption — the
/// single consumer takes ownership and reads from it directly.
#[derive(Clone)]
pub struct MessageBus {
    inbound_tx: Sender<InboundMessage>,
    outbound_tx: Sender<OutboundMessage>,
    inbound_rx: Arc<Mutex<Option<Receiver<InboundMessage>>>>,
    outbound_rx: Arc<Mutex<Option<Receiver<OutboundMessage>>>>,
}

impl MessageBus {
    /// Create a new message bus
    pub fn new(buffer_size: usize) -> Self {
        let (inbound_tx, inbound_rx) = channel(buffer_size);
        let (outbound_tx, outbound_rx) = channel(buffer_size);

        Self {
            inbound_tx,
            inbound_rx: Arc::new(Mutex::new(Some(inbound_rx))),
            outbound_tx,
            outbound_rx: Arc::new(Mutex::new(Some(outbound_rx))),
        }
    }

    /// Publish an inbound message
    pub async fn publish_inbound(&self, msg: InboundMessage) {
        if let Err(e) = self.inbound_tx.send(msg).await {
            tracing::error!("Failed to publish inbound message: {}", e);
        }
    }

    /// Take ownership of the inbound receiver.
    ///
    /// This can only be called once — the single consumer (e.g. AgentLoop)
    /// takes the receiver and reads from it in its own loop. Returns `None`
    /// if the receiver was already taken.
    pub async fn take_inbound_receiver(&self) -> Option<Receiver<InboundMessage>> {
        self.inbound_rx.lock().await.take()
    }

    /// Publish an outbound message
    pub async fn publish_outbound(&self, msg: OutboundMessage) {
        if let Err(e) = self.outbound_tx.send(msg).await {
            tracing::error!("Failed to publish outbound message: {}", e);
        }
    }

    /// Take ownership of the outbound receiver.
    ///
    /// This can only be called once — the single consumer (e.g. ChannelManager)
    /// takes the receiver and reads from it in its own loop. Returns `None`
    /// if the receiver was already taken.
    pub async fn take_outbound_receiver(&self) -> Option<Receiver<OutboundMessage>> {
        self.outbound_rx.lock().await.take()
    }

    /// Get a sender for inbound messages
    pub fn inbound_sender(&self) -> Sender<InboundMessage> {
        self.inbound_tx.clone()
    }

    /// Get a sender for outbound messages
    pub fn outbound_sender(&self) -> Sender<OutboundMessage> {
        self.outbound_tx.clone()
    }
}

impl Default for MessageBus {
    fn default() -> Self {
        Self::new(100)
    }
}
