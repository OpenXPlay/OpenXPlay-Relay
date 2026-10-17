// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16

//! Event-driven wake-ups for the controller's Bluetooth waits. BlueZ announces when an adapter
//! becomes usable and when a device connects (an iPhone that comes back into range pages the
//! accessory), so a wait for the adapter or the iPhone ends at once instead of running out its
//! backoff. Without a change, or without a change source, the waits still poll on their timers.
use crate::Result;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex, Weak,
};

/// A BlueZ change that can let a waiting attempt succeed now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BluetoothChange {
    /// An adapter appeared powered, was powered on or got its address.
    AdapterReady,
    /// The device with this address (upper case) connected.
    DeviceConnected(String),
}
impl BluetoothChange {
    /// The wake-up reason the controller logs.
    pub fn reason(&self) -> String {
        match self {
            Self::AdapterReady => "adapter-ready".into(),
            Self::DeviceConnected(address) => format!("device-connected address={address}"),
        }
    }
}

/// Receives BlueZ changes on the source's thread; it must not block.
pub type BluetoothListener = Arc<dyn Fn(&BluetoothChange) + Send + Sync>;

/// A source of BlueZ changes: the system bus on the device, a fake in tests.
pub trait BluetoothSignals: Send + Sync {
    /// Delivers every later change to `deliver` for the life of the process. An error means
    /// that no change will arrive; the waits then only poll.
    fn start(&self, deliver: BluetoothListener) -> Result<()>;
}

/// Fans the changes of one source out to the waits of every controller the relay runs, so the
/// source and its thread outlive controller restarts.
#[derive(Default)]
pub struct BluetoothWake {
    listeners: Mutex<Vec<(u64, BluetoothListener)>>,
    next: AtomicU64,
}
impl BluetoothWake {
    /// Connects `signals` to this hub; the source keeps no strong reference to it.
    pub fn listen(self: &Arc<Self>, signals: &dyn BluetoothSignals) -> Result<()> {
        let hub = Arc::downgrade(self);
        signals.start(Arc::new(move |change| {
            if let Some(hub) = hub.upgrade() {
                hub.publish(change);
            }
        }))
    }
    /// Passes `change` to every subscriber, outside the hub's lock.
    pub fn publish(&self, change: &BluetoothChange) {
        let listeners = self
            .listeners
            .lock()
            .unwrap()
            .iter()
            .map(|(_, listener)| listener.clone())
            .collect::<Vec<_>>();
        for listener in listeners {
            listener(change);
        }
    }
    /// Subscribes `listener` until the returned subscription is dropped.
    pub fn subscribe(self: &Arc<Self>, listener: BluetoothListener) -> BluetoothSubscription {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        self.listeners.lock().unwrap().push((id, listener));
        BluetoothSubscription {
            hub: Arc::downgrade(self),
            id,
        }
    }
}

/// Ends its subscription when dropped.
pub struct BluetoothSubscription {
    hub: Weak<BluetoothWake>,
    id: u64,
}
impl Drop for BluetoothSubscription {
    fn drop(&mut self) {
        if let Some(hub) = self.hub.upgrade() {
            hub.listeners
                .lock()
                .unwrap()
                .retain(|(id, _)| *id != self.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    /// Hands the deliver callback to the test, like a bus thread would use it.
    #[derive(Default)]
    struct Source(Mutex<Option<BluetoothListener>>);
    impl BluetoothSignals for Source {
        fn start(&self, deliver: BluetoothListener) -> Result<()> {
            *self.0.lock().unwrap() = Some(deliver);
            Ok(())
        }
    }
    fn counter(hub: &Arc<BluetoothWake>) -> (Arc<AtomicUsize>, BluetoothSubscription) {
        let count = Arc::new(AtomicUsize::new(0));
        let copy = count.clone();
        let subscription = hub.subscribe(Arc::new(move |_| {
            copy.fetch_add(1, Ordering::SeqCst);
        }));
        (count, subscription)
    }
    #[test]
    fn changes_reach_every_subscriber_until_it_unsubscribes() {
        let source = Source::default();
        let hub = Arc::new(BluetoothWake::default());
        hub.listen(&source).unwrap();
        let deliver = source.0.lock().unwrap().clone().unwrap();
        let (first, first_subscription) = counter(&hub);
        let (second, _second_subscription) = counter(&hub);
        deliver(&BluetoothChange::AdapterReady);
        assert_eq!(first.load(Ordering::SeqCst), 1);
        assert_eq!(second.load(Ordering::SeqCst), 1);
        drop(first_subscription);
        deliver(&BluetoothChange::DeviceConnected(
            "AA:BB:CC:DD:EE:01".into(),
        ));
        assert_eq!(first.load(Ordering::SeqCst), 1);
        assert_eq!(second.load(Ordering::SeqCst), 2);
        // The source holds no strong reference: a dropped hub silences it.
        drop(hub);
        deliver(&BluetoothChange::AdapterReady);
        assert_eq!(second.load(Ordering::SeqCst), 2);
    }
    #[test]
    fn a_listener_may_unsubscribe_others_while_a_change_is_delivered() {
        let hub = Arc::new(BluetoothWake::default());
        let (count, subscription) = counter(&hub);
        let held = Mutex::new(Some(subscription));
        let _dropper = hub.subscribe(Arc::new(move |_| {
            held.lock().unwrap().take();
        }));
        hub.publish(&BluetoothChange::AdapterReady);
        hub.publish(&BluetoothChange::AdapterReady);
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert_eq!(
            BluetoothChange::DeviceConnected("AA:BB:CC:DD:EE:01".into()).reason(),
            "device-connected address=AA:BB:CC:DD:EE:01"
        );
    }
}
