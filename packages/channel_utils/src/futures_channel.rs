use std::{
    ops::Deref,
    pin::{pin, Pin},
    sync::{atomic::AtomicBool, Arc, RwLock},
    task::{Context, Poll},
};

use futures_channel::mpsc::{TrySendError, UnboundedReceiver, UnboundedSender};
use futures_core::{FusedStream, Stream};

use crate::MoosicBoxSender;

pub struct MoosicBoxUnboundedSender<T: Send> {
    inner: UnboundedSender<T>,
    #[allow(clippy::type_complexity)]
    priority: Option<Arc<Box<dyn (Fn(&T) -> usize) + Send + Sync>>>,
    buffer: Arc<RwLock<Vec<(usize, T)>>>,
    ready_to_send: Arc<AtomicBool>,
}

impl<T: Send> MoosicBoxUnboundedSender<T> {
    pub fn with_priority(mut self, func: impl (Fn(&T) -> usize) + Send + Sync + 'static) -> Self {
        self.priority.replace(Arc::new(Box::new(func)));
        self
    }

    fn flush(&self) -> Result<(), TrySendError<T>> {
        let empty_buffer = { self.buffer.read().unwrap().is_empty() };
        if empty_buffer {
            log::debug!("flush: already empty");
            self.ready_to_send
                .store(true, std::sync::atomic::Ordering::SeqCst);
            return Ok(());
        }

        let mut buffer = self.buffer.write().unwrap();

        let (priority, item) = buffer.remove(0);
        let remaining_buffer_len = buffer.len();

        drop(buffer);

        log::debug!(
            "flush: sending buffered item with priority={priority} remaining_buf_len={remaining_buffer_len}",
        );

        self.unbounded_send(item)?;

        Ok(())
    }
}

impl<T: Send> MoosicBoxSender<T, TrySendError<T>> for MoosicBoxUnboundedSender<T> {
    fn send(&self, msg: T) -> Result<(), TrySendError<T>> {
        if !self
            .ready_to_send
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            if let Some(priority) = &self.priority {
                let priority = priority(&msg);

                let mut buffer = self.buffer.write().unwrap();

                let index =
                    buffer
                        .iter()
                        .enumerate()
                        .find_map(|(i, (p, _item))| if priority > *p { Some(i) } else { None });

                if let Some(index) = index {
                    buffer.insert(index, (priority, msg));
                } else {
                    buffer.push((priority, msg));
                }

                return Ok(());
            }
        }

        self.unbounded_send(msg)?;

        Ok(())
    }
}

impl<T: Send> Clone for MoosicBoxUnboundedSender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            priority: self.priority.clone(),
            buffer: self.buffer.clone(),
            ready_to_send: self.ready_to_send.clone(),
        }
    }
}

impl<T: Send> MoosicBoxUnboundedSender<T> {
    pub fn unbounded_send(&self, msg: T) -> Result<(), TrySendError<T>> {
        self.inner.unbounded_send(msg)
    }
}

impl<T: Send> Deref for MoosicBoxUnboundedSender<T> {
    type Target = UnboundedSender<T>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

pub struct MoosicBoxUnboundedReceiver<T: Send> {
    inner: UnboundedReceiver<T>,
    sender: MoosicBoxUnboundedSender<T>,
}

impl<T: Send> Deref for MoosicBoxUnboundedReceiver<T> {
    type Target = UnboundedReceiver<T>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T: Send> FusedStream for MoosicBoxUnboundedReceiver<T> {
    fn is_terminated(&self) -> bool {
        self.inner.is_terminated()
    }
}

impl<T: Send> Stream for MoosicBoxUnboundedReceiver<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        let this = self.get_mut();
        let inner = &mut this.inner;
        let stream = pin!(inner);
        let poll = stream.poll_next(cx);

        if let std::task::Poll::Ready(Some(_)) = &poll {
            if let Err(e) = this.sender.flush() {
                moosicbox_assert::die_or_error!("Failed to flush sender: {e:?}");
            }
        }

        poll
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

pub fn unbounded<T: Send>() -> (MoosicBoxUnboundedSender<T>, MoosicBoxUnboundedReceiver<T>) {
    let (tx, rx) = futures_channel::mpsc::unbounded();
    let ready_to_send = Arc::new(AtomicBool::new(true));

    let tx = MoosicBoxUnboundedSender {
        inner: tx,
        priority: None,
        buffer: Arc::new(RwLock::new(vec![])),
        ready_to_send: ready_to_send.clone(),
    };

    let rx = MoosicBoxUnboundedReceiver {
        inner: rx,
        sender: tx.clone(),
    };

    (tx, rx)
}
