//! Icon cache of the app (task T1.6), shared by every window and tab.
//!
//! The list asks only for the icons of the rows it draws; fx-shell extracts
//! them on its STA thread and answers with PNG bytes, which GPUI decodes off
//! the UI thread. Until an icon arrives the row shows an empty slot of the same
//! size, so nothing moves when it does. `FX_ICONS=0` turns icons off (to
//! compare benchmarks).

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::channel;

use fx_shell::icons::{IconKey, IconService};
use gpui::{Context, Image, ImageFormat};

#[derive(Debug, Clone)]
enum Slot {
    Pending,
    Ready(Arc<Image>),
    /// No icon (failed or timed out): not asked again.
    Missing,
}

pub struct Icons {
    service: Option<IconService>,
    cache: HashMap<IconKey, Slot>,
}

impl Icons {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let enabled = std::env::var_os("FX_ICONS").is_none_or(|v| v != "0");
        let service = if enabled { Self::start(cx) } else { None };
        Self {
            service,
            cache: HashMap::new(),
        }
    }

    pub fn enabled(&self) -> bool {
        self.service.is_some()
    }

    fn start(cx: &mut Context<Self>) -> Option<IconService> {
        let (tx, rx) = channel();
        let service = fx_shell::start_icon_service(tx).ok()?;
        cx.spawn(async move |this, cx| {
            let background = cx.background_executor().clone();
            let mut rx = rx;
            loop {
                // Wait on a background thread, never on the UI thread.
                let (back, ready) = background
                    .spawn(async move {
                        let r = rx.recv();
                        (rx, r)
                    })
                    .await;
                rx = back;
                let Ok(ready) = ready else { break };
                let alive = this.update(cx, |icons, cx| {
                    let slot = match ready.png {
                        Some(png) => {
                            Slot::Ready(Arc::new(Image::from_bytes(ImageFormat::Png, png.to_vec())))
                        }
                        None => Slot::Missing,
                    };
                    icons.cache.insert(ready.key, slot);
                    cx.notify();
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
        Some(service)
    }

    /// The icon for `key`, if it has arrived.
    pub fn get(&self, key: &IconKey) -> Option<Arc<Image>> {
        match self.cache.get(key) {
            Some(Slot::Ready(image)) => Some(image.clone()),
            _ => None,
        }
    }

    /// Whether `key` was never asked for.
    pub fn is_unknown(&self, key: &IconKey) -> bool {
        !self.cache.contains_key(key)
    }

    /// Asks for every key not asked before.
    pub fn request(&mut self, keys: impl IntoIterator<Item = IconKey>) {
        let Some(service) = &self.service else { return };
        for key in keys {
            if !self.cache.contains_key(&key) {
                self.cache.insert(key.clone(), Slot::Pending);
                service.request(key);
            }
        }
    }
}
