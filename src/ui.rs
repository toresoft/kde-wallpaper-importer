use std::process::Command;

/// Interazione con l'utente. Dietro trait perché i test non devono aprire
/// finestre: `SilentUi` e `testing::FakeUi` la sostituiscono.
pub trait Ui {
    /// Domanda bloccante sì/no. `true` significa «procedi».
    fn confirm(&self, title: &str, message: &str) -> bool;
    /// Notifica non bloccante di esito.
    fn notify(&self, summary: &str, body: &str, critical: bool);
}

/// Implementazione reale su `kdialog` e `notify-send`.
pub struct KdeUi;

impl Ui for KdeUi {
    fn confirm(&self, title: &str, message: &str) -> bool {
        match Command::new("kdialog")
            .arg("--title")
            .arg(title)
            .arg("--yesno")
            .arg(message)
            .status()
        {
            Ok(s) => s.success(),
            Err(e) => {
                eprintln!("kdialog non disponibile ({e}): procedo senza chiedere");
                true
            }
        }
    }

    fn notify(&self, summary: &str, body: &str, critical: bool) {
        let urgenza = if critical { "critical" } else { "normal" };
        let _ = Command::new("notify-send")
            .args([
                "-a",
                "Wallpaper Importer",
                "-i",
                "preferences-desktop-wallpaper",
                "-u",
                urgenza,
            ])
            .arg(summary)
            .arg(body)
            .status();
    }
}

/// Usata con `--no-ui`: nessun dialogo, nessuna notifica, conferma implicita.
pub struct SilentUi;

impl Ui for SilentUi {
    fn confirm(&self, _title: &str, _message: &str) -> bool {
        true
    }
    fn notify(&self, _summary: &str, _body: &str, _critical: bool) {}
}

#[cfg(test)]
pub mod testing {
    use super::Ui;
    use std::cell::RefCell;

    /// UI finta che registra ogni interazione.
    pub struct FakeUi {
        risposta: bool,
        confirms: RefCell<Vec<String>>,
        notifications: RefCell<Vec<(String, String, bool)>>,
    }

    impl FakeUi {
        pub fn new(risposta_confirm: bool) -> Self {
            Self {
                risposta: risposta_confirm,
                confirms: RefCell::new(Vec::new()),
                notifications: RefCell::new(Vec::new()),
            }
        }

        pub fn confirms(&self) -> Vec<String> {
            self.confirms.borrow().clone()
        }

        pub fn notifications(&self) -> Vec<(String, String, bool)> {
            self.notifications.borrow().clone()
        }
    }

    impl Ui for FakeUi {
        fn confirm(&self, _title: &str, message: &str) -> bool {
            self.confirms.borrow_mut().push(message.to_string());
            self.risposta
        }

        fn notify(&self, summary: &str, body: &str, critical: bool) {
            self.notifications
                .borrow_mut()
                .push((summary.to_string(), body.to_string(), critical));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::FakeUi;
    use super::*;

    #[test]
    fn silent_ui_conferma_sempre_e_non_notifica() {
        let ui = SilentUi;
        assert!(ui.confirm("titolo", "messaggio"));
        ui.notify("s", "b", true); // non deve fare nulla né andare in panico
    }

    #[test]
    fn fake_ui_registra_conferme_e_notifiche() {
        let ui = FakeUi::new(false);
        assert!(!ui.confirm("t", "davvero?"));
        ui.notify("Esito", "1 importati", false);

        assert_eq!(ui.confirms(), vec!["davvero?".to_string()]);
        assert_eq!(
            ui.notifications(),
            vec![("Esito".to_string(), "1 importati".to_string(), false)]
        );
    }
}
