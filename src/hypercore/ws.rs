use std::{
    collections::HashSet,
    pin::Pin,
    task::{Context, Poll, ready},
    time::Duration,
};

use anyhow::Result;
use futures::StreamExt;
use tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
    time::{interval, sleep, timeout},
};
use url::Url;
use yawc::{Options, WebSocket};

use crate::hypercore::types::{Incoming, Outgoing, Subscription};

struct Stream {
    stream: WebSocket,
}

impl Stream {
    /// Establish a WebSocket connection.
    async fn connect(url: Url) -> Result<Self> {
        let stream = yawc::WebSocket::connect(url)
            .with_options(Options::default().with_no_delay())
            .await?;

        Ok(Self { stream })
    }

    /// Subscribes to a topic.
    async fn subscribe(&mut self, subscription: Subscription) -> anyhow::Result<()> {
        self.stream
            .send_json(&Outgoing::Subscribe { subscription })
            .await?;
        Ok(())
    }

    /// Unsubscribes from a topic.
    async fn unsubscribe(&mut self, subscription: Subscription) -> anyhow::Result<()> {
        self.stream
            .send_json(&Outgoing::Unsubscribe { subscription })
            .await?;
        Ok(())
    }

    /// Send a ping
    async fn ping(&mut self) -> anyhow::Result<()> {
        self.stream.send_json(&Outgoing::Ping).await?;
        Ok(())
    }
}

impl futures::Stream for Stream {
    type Item = Incoming;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        while let Some(item) = ready!(this.stream.poll_next_unpin(cx)) {
            match serde_json::from_slice(&item.payload) {
                Ok(ok) => {
                    return Poll::Ready(Some(ok));
                }
                Err(err) => {
                    if let Ok(s) = std::str::from_utf8(&item.payload) {
                        log::warn!("unable to parse: {}: {:?}", s, err);
                    }
                }
            }
        }

        Poll::Ready(None)
    }
}

type SubChannelData = (bool, Subscription);

/// Persistent WebSocket connection.
pub struct Connection {
    rx: UnboundedReceiver<Incoming>,
    // TODO: oneshot??
    tx: UnboundedSender<SubChannelData>,
}

impl Connection {
    /// Creates a new reconnecting websocket connection.
    pub fn new(url: Url) -> Self {
        let (tx, rx) = unbounded_channel();
        let (stx, srx) = unbounded_channel();
        tokio::spawn(connection(url, tx, srx));
        Self { rx, tx: stx }
    }

    /// Subscribes to a topic.
    pub fn subscribe(&self, subscription: Subscription) {
        let _ = self.tx.send((true, subscription));
    }

    /// Unsubscribes from a topic.
    pub fn unsubscribe(&self, subscription: Subscription) {
        let _ = self.tx.send((false, subscription));
    }

    /// Close the websocket connection.
    pub fn close(self) {
        drop(self);
    }
}

impl futures::Stream for Connection {
    type Item = Incoming;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        this.rx.poll_recv(cx)
    }
}

async fn connection(
    url: Url,
    tx: UnboundedSender<Incoming>,
    mut srx: UnboundedReceiver<SubChannelData>,
) {
    let mut subs = HashSet::new();

    loop {
        let mut stream = match timeout(Duration::from_secs(5), Stream::connect(url.clone())).await {
            Ok(ok) => match ok {
                Ok(ok) => ok,
                Err(err) => {
                    log::error!("unable to connect to {url}: {err:?}");
                    sleep(Duration::from_millis(1_500)).await;
                    continue;
                }
            },
            Err(err) => {
                log::error!("timed out connecting to {url}: {err:?}");
                sleep(Duration::from_millis(1_500)).await;
                continue;
            }
        };

        // Initial subscription
        for sub in subs.iter().cloned() {
            log::debug!("Initial subscription to {sub}");
            let _ = stream.subscribe(sub).await;
        }

        let mut ping = interval(Duration::from_secs(5));
        loop {
            tokio::select! {
                _ = ping.tick() => {
                    let _ = stream.ping().await;
                }
                maybe_item = stream.next() => {
                    let Some(item) = maybe_item else { break; };
                    let _ = tx.send(item);
                }
                item = srx.recv() => {
                    let Some((is_sub, sub)) = item else { return };
                    if is_sub {
                        if !subs.insert(sub.clone()) {
                            log::debug!("Already subscribed to {sub:?}");
                            continue;
                        }

                        if let Err(err) = stream.subscribe(sub).await {
                            log::error!("Subscribing: {err:?}");
                            break;
                        }
                    } else if subs.remove(&sub) {
                        // ...
                        if let Err(err) = stream.unsubscribe(sub).await {
                            log::error!("Unsubscribing: {err:?}");
                            break;
                        }
                    }
                }
            }
        }

        log::debug!("Disconnected from {url}");
    }
}
