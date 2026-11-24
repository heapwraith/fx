//! Commands of a folder view that reach the system: the index service, the
//! default file manager, cloud files, updates and crash reports (from the
//! palette and the status bar).

use super::*;

impl FolderView {
    /// Folders this tab visited before the current one, newest first
    /// (recent folders of the command palette).
    pub fn recent_folders(&self) -> Vec<PathBuf> {
        let (back, _, forward) = self.history.parts();
        back.iter()
            .rev()
            .chain(forward.iter().rev())
            .map(|v| v.path.clone())
            .collect()
    }

    /// Installs or removes `fx-indexd` (next to `fx.exe`) as Administrator:
    /// Windows shows its UAC prompt; the result comes back as a notice (T2.5).
    pub fn index_service(&mut self, install: bool, cx: &mut Context<Self>) {
        let exe = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("fx-indexd.exe")))
            .filter(|p| p.exists());
        let Some(exe) = exe else {
            self.show_notice(
                tr!(
                    "fx-indexd.exe não está ao lado do pathdash.exe",
                    "fx-indexd.exe is not next to pathdash.exe"
                )
                .to_owned(),
                cx,
            );
            return;
        };
        let args = if install { "install" } else { "uninstall" }.to_owned();
        let rx = fx_shell::launch(Launch::Elevated { exe, args });
        self.show_notice(
            tr!(
                "Aguardando a permissão de administrador do Windows…",
                "Waiting for Windows administrator permission…"
            )
            .to_owned(),
            cx,
        );
        cx.spawn(async move |this, cx| {
            let result = crate::chan::next(rx).await.unwrap_or(Ok(()));
            let text = match (result, install) {
                (Ok(()), true) => tr!(
                    "Busca no disco inteiro ativada: o índice fica pronto em alguns segundos",
                    "Whole-disk search is on: the index will be ready in a few seconds"
                )
                .to_owned(),
                (Ok(()), false) => {
                    tr!("Serviço de índice removido", "Index service removed").to_owned()
                }
                (Err(e), _) => trf!("serviço de índice: {e}", "index service: {e}"),
            };
            let _ = this.update(cx, |view, cx| view.show_notice(text, cx));
        })
        .detach();
    }

    /// Makes fx the default file manager, or goes back to Explorer (T3.1):
    /// keys under `HKCU` only, on a background thread, with a full revert.
    pub fn default_manager(&mut self, enable: bool, cx: &mut Context<Self>) {
        let Some(exe) = std::env::current_exe().ok() else {
            return;
        };
        let rx = fx_shell::launch::spawn_launch(move || {
            use fx_shell::default_manager::{BACKUP, CLASSES, enable as on, revert};
            if enable {
                on(CLASSES, BACKUP, &exe.to_string_lossy())
            } else {
                revert(CLASSES, BACKUP)
            }
        });
        cx.spawn(async move |this, cx| {
            let result = crate::chan::next(rx).await.unwrap_or(Ok(()));
            let text = match (result, enable) {
                (Ok(()), true) => tr!(
                    "O Pathdash agora abre pastas, unidades e o Win+E. \"Voltar ao Explorador\" desfaz",
                    "Pathdash now opens folders, drives and Win+E. \"Back to Explorer\" undoes it"
                )
                .to_owned(),
                (Ok(()), false) => tr!(
                    "O Explorador voltou a ser o gerenciador padrão",
                    "Explorer is the default file manager again"
                )
                .to_owned(),
                (Err(e), _) => trf!("gerenciador padrão: {e}", "default manager: {e}"),
            };
            let _ = this.update(cx, |view, cx| view.show_notice(text, cx));
        })
        .detach();
    }

    /// The focused view is using Tab itself (path field, search, batch
    /// rename): the dual pane must not take it (T3.2).
    pub fn uses_tab(&self) -> bool {
        self.edit.is_some() || self.search.is_some() || self.batch_rename.is_some()
    }

    /// The items an operation would act on (the selection, or the cursor).
    pub fn selected_paths(&self) -> Vec<PathBuf> {
        self.targets()
    }

    /// Queues `job` like a key of this view would (queue, conflicts, undo).
    pub fn queue_job(&mut self, job: fx_ops::Job, window: &mut Window, cx: &mut Context<Self>) {
        let _ = self.submit_job(job, window, cx);
    }

    /// A message in the status until the next key (palette commands).
    /// "Always keep on this device" (`keep`) or "Free up space" for the
    /// selection (T3.9), on a worker thread: only attributes change, the
    /// cloud provider downloads or frees the data.
    pub fn cloud_pin(&mut self, keep: bool, cx: &mut Context<Self>) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            self.show_notice(tr!("nada selecionado", "nothing selected").to_owned(), cx);
            return;
        }
        let work = cx.background_executor().spawn(async move {
            let mut done = 0usize;
            let mut error = None;
            for p in &paths {
                match fx_fs::set_cloud_pin(p, keep) {
                    Ok(n) => done += n,
                    Err(e) => error = Some(e),
                }
            }
            (done, error)
        });
        cx.spawn(async move |this, cx| {
            let (done, error) = work.await;
            let _ = this.update(cx, |view, cx| {
                let n = fx_core::format::items(done);
                let text = match (error, keep) {
                    (Some(e), _) => trf!("nuvem: {e}", "cloud: {e}"),
                    (None, true) => trf!(
                        "{n} marcados para manter neste dispositivo",
                        "{n} marked to always keep on this device"
                    ),
                    (None, false) => trf!(
                        "{n} marcados para liberar espaço",
                        "{n} marked to free up space"
                    ),
                };
                view.show_notice(text, cx);
            });
        })
        .detach();
    }

    /// At startup: an update of Pathdash does not reach the index service
    /// (it needs Administrator), so a service older than this Pathdash is
    /// pointed out once, with where to update it. Reads files: a worker.
    pub fn check_index_service(&mut self, cx: &mut Context<Self>) {
        let work = cx.background_executor().spawn(async move {
            fx_index::service::service_state() == fx_index::service::ServiceState::Running
                && crate::update::index_outdated()
        });
        cx.spawn(async move |this, cx| {
            if work.await {
                let _ = this.update(cx, |view, cx| {
                    if view.notice.is_none() {
                        view.show_notice(
                            tr!(
                                "Serviço do índice desatualizado · Configurações › Sistema",
                                "Index service out of date · Settings › System"
                            )
                            .to_owned(),
                            cx,
                        );
                    }
                });
            }
        })
        .detach();
    }

    /// The update check (T4.1/T4.4), on a worker thread: a newer version is
    /// downloaded, checked and kept ready, applied when fx closes. `quiet`:
    /// the daily check at startup, which only speaks when one is ready.
    pub fn check_updates(&mut self, quiet: bool, cx: &mut Context<Self>) {
        let check = move || -> std::io::Result<Option<fx_core::update::Version>> {
            if let Some((v, _, _)) = crate::update::ready() {
                return Ok(Some(v));
            }
            if quiet && !crate::update::due() {
                return Ok(None);
            }
            match crate::update::check()? {
                Some(u) if !quiet || crate::update::automatic() => {
                    crate::update::prepare(&u)?;
                    Ok(Some(u.version))
                }
                _ => Ok(None),
            }
        };
        // The network and a download of up to 200 MB: a thread of its own,
        // never the background pool (review, D-066).
        let (tx, rx) = futures::channel::oneshot::channel();
        let _ = std::thread::Builder::new()
            .name("fx-update".into())
            .spawn(move || {
                let _ = tx.send(check());
            });
        cx.spawn(async move |this, cx| {
            let result = rx.await.unwrap_or_else(|_| {
                Err(std::io::Error::other(tr!(
                    "não foi possível começar",
                    "could not start"
                )))
            });
            if let Err(e) = &result {
                log::warn!("update check failed: {e}");
            }
            let _ = this.update(cx, |view, cx| {
                let text = match result {
                    Ok(Some(v)) => {
                        let v = v.to_string();
                        trf!(
                            "Versão {v} pronta · entra quando você fechar o Pathdash (Ctrl+K: reiniciar agora)",
                            "Version {v} ready · applies when you close Pathdash (Ctrl+K: restart now)"
                        )
                    }
                    _ if quiet => return,
                    Ok(None) => {
                        tr!("O Pathdash já está na versão mais recente", "Pathdash is up to date").to_owned()
                    }
                    Err(e) => trf!(
                        "Não foi possível atualizar: {e}",
                        "Could not update: {e}"
                    ),
                };
                view.show_notice(text, cx);
            });
        })
        .detach();
    }

    /// Palette "Atualizar o fx": restarts into the version ready (saving the
    /// session first), or looks for one now.
    pub fn update_command(&mut self, cx: &mut Context<Self>) {
        let work = cx
            .background_executor()
            .spawn(async move { crate::update::ready().is_some() });
        cx.spawn(async move |this, cx| {
            let ready = work.await;
            // At the app level, not inside this tab's update: saving the
            // session reads every tab, this one included.
            if ready {
                let restarted = cx
                    .update(|cx| {
                        if let Some(p) = cx.try_global::<crate::persist::PersistGlobal>() {
                            let p = p.0.clone();
                            p.update(cx, |p, cx| p.flush(cx));
                        }
                        let started = crate::update::apply(true);
                        if started {
                            cx.quit();
                        }
                        started
                    })
                    .unwrap_or(false);
                if restarted {
                    return;
                }
            }
            let _ = this.update(cx, |view, cx| {
                view.show_notice(
                    tr!("Procurando atualizações…", "Checking for updates…").to_owned(),
                    cx,
                );
                view.check_updates(false, cx);
            });
        })
        .detach();
    }

    /// Palette: opens the issue page for the newest crash report (T3.10).
    /// Reading the small report and starting the browser run off the UI
    /// thread; the report is then marked as seen.
    pub fn report_crash(&mut self, cx: &mut Context<Self>) {
        let work = cx.background_executor().spawn(async move {
            let path = crate::crash::pending()?;
            let report = std::fs::read_to_string(&path).ok()?;
            let url = crate::crash::issue_url(&report);
            Some((path, url))
        });
        cx.spawn(async move |this, cx| {
            let found = work.await;
            let _ = this.update(cx, |view, cx| match found {
                Some((path, url)) => {
                    view.launch(Launch::Open(PathBuf::from(url)), cx);
                    // A rename on disk: not on the UI thread.
                    cx.background_spawn(async move { crate::crash::mark_seen(&path) })
                        .detach();
                    view.show_notice(
                        tr!(
                            "Página da issue aberta no navegador: confira o texto e envie se quiser",
                            "Issue page opened in the browser: check the text and send it if you want"
                        )
                        .to_owned(),
                        cx,
                    );
                }
                None => view.show_notice(
                    tr!("Nenhuma falha para relatar", "No crash to report").to_owned(),
                    cx,
                ),
            });
        })
        .detach();
    }
}
