use futures::Stream;
use helius::types::TransactionNotification;
use pin_project::pin_project;
use std::{
    collections::VecDeque,
    pin::Pin,
    task::{Context, Poll},
};

pub enum Item<T> {
    FromStream(T),
    Injected(u64),
}

pub trait GiveSlot {
    fn slot(&self) -> u64;
}
impl GiveSlot for TransactionNotification {
    fn slot(&self) -> u64 {
        self.slot
    }
}

impl<T> Item<T>
where
    T: GiveSlot,
{
    pub fn slot(&self) -> u64 {
        match self {
            Self::FromStream(item) => item.slot(),
            Self::Injected(slot) => *slot,
        }
    }
}

#[pin_project]
pub struct WrappedStream<S> {
    #[pin]
    inner: S,
    inject_queue: VecDeque<u64>,
}

impl<S> WrappedStream<S> {
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            inject_queue: VecDeque::new(),
        }
    }

    pub fn inject(&mut self, val: u64) {
        self.inject_queue.push_back(val);
    }
}

impl<S: Stream> Stream for WrappedStream<S> {
    type Item = Item<S::Item>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();

        if let Some(val) = this.inject_queue.pop_front() {
            return Poll::Ready(Some(Item::Injected(val)));
        }

        this.inner
            .poll_next(cx)
            .map(|opt| opt.map(Item::FromStream))
    }
}
