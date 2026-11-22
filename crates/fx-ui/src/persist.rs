//! Saving the session (task T1.17). Every 2 s the app's windows are asked
//! for their state; when it changed, it goes to the store's writer thread.
//! Closing a window drops it from the session, except the last one, whose
//! state is written right away (it is what the next start restores).
//!
//! Disabled for benchmarks (`FX_TRACE`) and with `FX_NO_STATE=1`, so test
//! runs never overwrite the user's session.

use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::Duration;

use gpui::{App, Context, Entity, WindowHandle, WindowId};

use crate::places::Places;
use crate::store::{self, Session, Settings, UiSettings, WindowState};
use crate::tabs_view::TabsView;

const EVERY: Duration = Duration::from_secs(2);

pub struct Persist {
    writer: Option<Sender<Session>>,
    files: Option<(PathBuf, PathBuf)>,
    windows: Vec<WindowHandle<TabsView>>,
    places: Entity<Places>,
    /// Loaded settings (keys the UI does not change are kept as they were).
    settings: Settings,
    last: Option<Session>,
}

impl Persist {
    pub fn new(
        enabled: bool,
        settings: Settings,
        places: Entity<Places>,
        cx: &mut Context<Self>,
    ) -> Self {
        let files = enabled
            .then(|| Some((store::state_path()?, store::settings_path()?)))
            .flatten();
        let writer = files
            .as_ref()
            .and_then(|(db, s)| store::spawn_writer(db.clone(), s.clone()).ok());
        if writer.is_some() {
            cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(EVERY).await;
                    if this.update(cx, |p, cx| p.save(None, cx)).is_err() {
                        break;
                    }
                }
            })
            .detach();
        }
        Self {
            writer,
            files,
            windows: Vec::new(),
            places,
            settings,
            last: None,
        }
    }

    pub fn track(&mut self, window: WindowHandle<TabsView>) {
        self.windows.push(window);
    }

    /// Window `id` is closing; `state` is its last state.
    pub fn closing(
        &mut self,
        id: WindowId,
        state: WindowState,
        settings: UiSettings,
        cx: &mut App,
    ) {
        if self.writer.is_none() {
            return;
        }
        if self.windows.len() > 1 {
            self.windows.retain(|w| w.window_id() != id);
            self.save(None, cx);
            return;
        }
        // The last window: write now, the process is about to end.
        let session = self.collect(Some((state, settings)), cx);
        if let Some((db, settings_file)) = &self.files
            && let Ok(mut store) = store::Db::open(db)
        {
            let _ = store.save(&session);
            let _ = store::save_settings(settings_file, &session.settings);
        }
    }

    fn collect(&mut self, closing: Option<(WindowState, UiSettings)>, cx: &mut App) -> Session {
        let mut windows = Vec::new();
        let mut ui = closing.as_ref().map(|(_, s)| s.clone());
        for handle in &self.windows {
            if let Ok((w, s)) = handle.update(cx, |view, window, cx| {
                (view.export(window, cx), view.settings_part())
            }) {
                ui.get_or_insert(s);
                windows.push(w);
            }
        }
        if let Some((state, _)) = closing {
            windows = vec![state];
        }
        let mut settings = self.settings.clone();
        if let Some(ui) = ui {
            settings.densidade = ui.density;
            settings.barra_lateral = ui.sidebar;
            settings.mostrar_ocultos = ui.show_hidden;
        }
        if crate::tabs_view::guide_seen() {
            settings.guia_visto = true;
        }
        if let Some(on) = crate::update::chosen() {
            settings.verificar_atualizacoes = on;
        }
        if let Some(on) = crate::crash::chosen() {
            settings.relatorio_falhas = on;
        }
        if let Some(lang) = crate::i18n::chosen() {
            settings.idioma = lang.to_owned();
        }
        let places = self.places.read(cx);
        let pinned = places
            .pinned_ready()
            .then(|| places.pinned.paths().to_vec());
        Session {
            windows,
            pinned,
            settings,
        }
    }

    fn save(&mut self, closing: Option<(WindowState, UiSettings)>, cx: &mut App) {
        let Some(writer) = self.writer.clone() else {
            return;
        };
        let session = self.collect(closing, cx);
        if self.last.as_ref() != Some(&session) {
            let _ = writer.send(session.clone());
            self.last = Some(session);
        }
    }
}

/// Reads the saved session (before the event loop starts).
pub fn load_session() -> Option<Session> {
    let db = store::Db::open(&store::state_path()?).ok()?;
    db.load().ok()
}
