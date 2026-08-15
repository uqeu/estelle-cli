//! App-level handlers for ambient terminal pet events.

use super::*;

impl App {
    pub(super) fn disable_ambient_pet_before_shutdown(&mut self, tui: &mut tui::Tui) -> Result<()> {
        self.chat_widget.disable_ambient_pet_for_session();
        if let Err(clear_err) = tui.clear_ambient_pet_image() {
            match clear_err {
                crate::pets::PetImageRenderError::Terminal(err) => return Err(err.into()),
                crate::pets::PetImageRenderError::Asset(err) => {
                    tracing::warn!(
                        error = %err,
                        "failed to clear ambient pet image before shutdown feedback"
                    );
                }
            }
        }
        Ok(())
    }

    pub(super) fn handle_ambient_pet_image_render_error(
        &mut self,
        tui: &mut tui::Tui,
        err: crate::pets::PetImageRenderError,
    ) -> Result<()> {
        match err {
            crate::pets::PetImageRenderError::Terminal(err) => Err(err.into()),
            crate::pets::PetImageRenderError::Asset(err) => {
                tracing::warn!(
                    error = %err,
                    "failed to render ambient pet image; disabling pet for session"
                );
                self.chat_widget.disable_ambient_pet_for_session();
                if let Err(clear_err) = tui.clear_ambient_pet_image() {
                    match clear_err {
                        crate::pets::PetImageRenderError::Terminal(err) => return Err(err.into()),
                        crate::pets::PetImageRenderError::Asset(err) => {
                            tracing::warn!(
                                error = %err,
                                "failed to clear ambient pet image after render failure"
                            );
                        }
                    }
                }
                Ok(())
            }
        }
    }

    pub(super) fn handle_configured_pet_loaded(
        &mut self,
        tui: &mut tui::Tui,
        pet_id: String,
        result: Result<Option<crate::pets::AmbientPet>, String>,
    ) {
        if self.config.tui_pet.as_deref() != Some(pet_id.as_str()) {
            return;
        }

        match result {
            Ok(ambient_pet) => {
                self.chat_widget
                    .set_tui_pet_loaded(Some(pet_id), ambient_pet);
                tui.frame_requester().schedule_frame();
            }
            Err(err) => {
                self.chat_widget
                    .add_warning_message(format!("Failed to load configured pet: {err}"));
            }
        }
    }
}
