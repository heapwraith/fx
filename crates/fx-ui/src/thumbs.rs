//! Thumbnail cache of the app (task T1.7), for the grid view.
//!
//! The grid tells [`Thumbs::want`] which items it shows; fx-shell's queue
//! follows that list (items scrolled away are cancelled) and answers with PNG
//! bytes that GPUI decodes off the UI thread. At most `LIMIT` thumbnails are
//! kept: the oldest are dropped, with their decoded copy in GPUI's asset
//! cache, and fetched again (usually from Windows' cache) if they come back
//! on screen.

use futures::StreamExt;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::channel;

use fx_shell::thumbs::ThumbService;
use gpui::{Context, Image, ImageFormat, ImageSource};

/// Thumbnails kept in memory (96 px: ~37 KB decoded each, ~15 MB in all).
const LIMIT: usize = 400;

enum Slot {
    Ready(Arc<Image>),
    /// No thumbnail and no icon: the grid shows the small icon.
    Missing,
}

pub struct Thumbs {
    /// Started on the first request (the list view never needs it).
    service: Option<ThumbService>,
    started: bool,
    cache: HashMap<PathBuf, Slot>,
    /// Arrival order of the cached items, oldest first.
    order: VecDeque<PathBuf>,
    /// Last list given to the service (skip repeating it every frame).
    wanted: Vec<PathBuf>,
}

impl Thumbs {
    pub fn new(_: &mut Context<Self>) -> Self {
        Self {
            service: None,
            started: false,
            cache: HashMap::new(),
            order: VecDeque::new(),
            wanted: Vec::new(),
        }
    }

    fn start(cx: &mut Context<Self>) -> Option<ThumbService> {
        let (tx, rx) = channel();
        let service = fx_shell::start_thumb_service(tx).ok()?;
        cx.spawn(async move |this, cx| {
            let mut rx = crate::chan::stream(rx);
            while let Some(ready) = rx.next().await {
                let alive = this.update(cx, |thumbs, cx| {
                    let slot = match ready.png {
                        Some(png) => {
                            Slot::Ready(Arc::new(Image::from_bytes(ImageFormat::Png, png.to_vec())))
                        }
                        None => Slot::Missing,
                    };
                    thumbs.insert(ready.path, slot, cx);
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

    fn insert(&mut self, path: PathBuf, slot: Slot, cx: &mut Context<Self>) {
        if self.cache.insert(path.clone(), slot).is_none() {
            self.order.push_back(path);
        }
        let mut forgotten = Vec::new();
        while self.order.len() > LIMIT {
            let Some(old) = self.order.pop_front() else {
                break;
            };
            if let Some(Slot::Ready(image)) = self.cache.remove(&old) {
                ImageSource::from(image).remove_asset(cx);
            }
            forgotten.push(old);
        }
        if let Some(service) = &self.service
            && !forgotten.is_empty()
        {
            service.forget(&forgotten);
        }
    }

    pub fn get(&self, path: &PathBuf) -> Option<Arc<Image>> {
        match self.cache.get(path) {
            Some(Slot::Ready(image)) => Some(image.clone()),
            _ => None,
        }
    }

    /// The items on screen, in order; those without a thumbnail yet are
    /// queued and everything else pending is cancelled.
    pub fn want(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        if !self.started {
            self.started = true;
            self.service = Self::start(cx);
        }
        let Some(service) = &self.service else { return };
        let missing: Vec<PathBuf> = paths
            .into_iter()
            .filter(|p| !self.cache.contains_key(p))
            .collect();
        if missing != self.wanted {
            service.want(missing.clone());
            self.wanted = missing;
        }
    }
}
