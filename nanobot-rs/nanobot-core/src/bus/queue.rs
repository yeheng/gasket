//! Message bus for inter-component communication

use std::sync::Arc;

use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::Mutex;

use super::events::{InboundMessage, OutboundMessage};

/// Message bus for routing messages between channels and agent
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

    /// Consume an inbound message
    pub async fn consume_inbound(&self) -> Option<InboundMessage> {
        let mut rx_guard = self.inbound_rx.lock().await;
        let rx = rx_guard.as_mut()?;
        rx.recv().await
    }

    /// Publish an outbound message
    pub async fn publish_outbound(&self, msg: OutboundMessage) {
        if let Err(e) = self.outbound_tx.send(msg).await {
            tracing::error!("Failed to publish outbound message: {}", e);
        }
    }

    /// Consume an outbound message
    pub async fn consume_outbound(&self) -> Option<OutboundMessage> {
        let mut rx_guard = self.outbound_rx.lock().await;
        let rx = rx_guard.as_mut()?;
        rx.recv().await
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
