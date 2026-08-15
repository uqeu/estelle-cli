//! Chat widget helpers for ambient terminal pets.
//!
//! The /pets picker is deleted (attack-11 egress audit, 2026-08-13): it depended on the
//! pets CDN asset fetch, which no longer exists. The ambient pet stays — it renders only
//! from the local cache and degrades to absent when no valid cached spritesheet exists.

use super::*;
use codex_config::types::TuiPetAnchor;

pub(super) fn start_configured_pet_load_if_needed(
    config: &Config,
    ambient_pet_missing: bool,
    frame_requester: FrameRequester,
    app_event_tx: AppEventSender,
    pet_http_client: codex_http_client::RouteAwareClientPool,
) {
    let Some(pet_id) = config.tui_pet.clone() else {
        return;
    };
    if pet_id == crate::pets::DISABLED_PET_ID || !ambient_pet_missing {
        return;
    }

    let codex_home = config.codex_home.clone();
    let animations_enabled = config.animations;
    let event_pet_id = pet_id.clone();
    spawn_pet_load(
        async move {
            crate::pets::load_pet_with_assets(
                pet_id,
                codex_home,
                frame_requester,
                animations_enabled,
                &pet_http_client,
            )
            .await
            .map(Some)
            .map_err(|err| err.to_string())
        },
        app_event_tx,
        move |result| AppEvent::ConfiguredPetLoaded {
            pet_id: event_pet_id,
            result,
        },
    );
}

#[cfg(test)]
pub(super) fn load_ambient_pet(
    config: &Config,
    frame_requester: FrameRequester,
) -> Option<crate::pets::AmbientPet> {
    let selected_pet = config.tui_pet.as_deref()?;
    if selected_pet == crate::pets::DISABLED_PET_ID {
        return None;
    }

    crate::pets::AmbientPet::load(
        Some(selected_pet),
        &config.codex_home,
        frame_requester,
        config.animations,
    )
    .ok()
}

impl ChatWidget {
    pub(super) fn set_ambient_pet_notification(
        &mut self,
        kind: crate::pets::PetNotificationKind,
        body: Option<String>,
    ) {
        if let Some(pet) = self.ambient_pet.as_mut() {
            pet.set_notification(kind, body);
        }
    }

    pub(crate) fn ambient_pet_image_enabled(&self) -> bool {
        self.ambient_pet
            .as_ref()
            .is_some_and(crate::pets::AmbientPet::image_enabled)
    }

    pub(crate) fn disable_ambient_pet_for_session(&mut self) {
        self.ambient_pet = None;
        self.request_redraw();
    }

    pub(crate) fn ambient_pet_draw(
        &self,
        area: Rect,
        composer_bottom_y: u16,
    ) -> Option<crate::pets::AmbientPetDraw> {
        if !self.bottom_pane.no_modal_or_popup_active() {
            return None;
        }

        let anchor_bottom_y = match self.config.tui_pet_anchor {
            TuiPetAnchor::Composer => composer_bottom_y,
            TuiPetAnchor::ScreenBottom => area.bottom(),
        };
        self.ambient_pet
            .as_ref()?
            .draw_request(area, anchor_bottom_y)
    }

    pub(super) fn ambient_pet_wrap_reserved_cols(&self) -> u16 {
        self.ambient_pet
            .as_ref()
            .filter(|pet| pet.image_enabled())
            .map(|pet| {
                pet.image_columns()
                    .saturating_add(AMBIENT_PET_WRAP_GAP_COLUMNS)
            })
            .unwrap_or(0)
    }

    pub(crate) fn history_wrap_width(&self, width: u16) -> u16 {
        width
            .saturating_sub(self.ambient_pet_wrap_reserved_cols())
            .max(1)
    }

    /// Set the ambient pet in the widget's config copy (test helper for the ambient path).
    #[cfg(test)]
    pub(crate) fn set_tui_pet(&mut self, pet: Option<String>) {
        self.config.tui_pet = pet;
        self.ambient_pet = load_ambient_pet(&self.config, self.frame_requester.clone());
        self.apply_ambient_pet_image_support_override_for_tests();
        self.request_redraw();
    }

    pub(crate) fn set_tui_pet_loaded(
        &mut self,
        pet: Option<String>,
        ambient_pet: Option<crate::pets::AmbientPet>,
    ) {
        self.config.tui_pet = pet;
        self.ambient_pet = ambient_pet;
        self.apply_ambient_pet_image_support_override_for_tests();
        self.request_redraw();
    }

    #[cfg(test)]
    fn apply_ambient_pet_image_support_override_for_tests(&mut self) {
        if let Some(support) = self.pet_image_support_override
            && let Some(pet) = self.ambient_pet.as_mut()
        {
            pet.set_image_support_for_tests(support);
        }
    }

    #[cfg(not(test))]
    fn apply_ambient_pet_image_support_override_for_tests(&mut self) {}

    #[cfg(test)]
    pub(crate) fn set_pet_image_support_for_tests(
        &mut self,
        support: crate::pets::PetImageSupport,
    ) {
        self.pet_image_support_override = Some(support);
        self.apply_ambient_pet_image_support_override_for_tests();
    }

    #[cfg(test)]
    pub(crate) fn install_test_ambient_pet_for_tests(&mut self, animations_enabled: bool) {
        self.set_tui_pet_loaded(
            Some("test".to_string()),
            Some(crate::pets::test_ambient_pet(
                self.frame_requester.clone(),
                animations_enabled,
            )),
        );
    }
}

fn spawn_pet_load<T>(
    future: impl std::future::Future<Output = Result<T, String>> + Send + 'static,
    app_event_tx: AppEventSender,
    completion_event: impl FnOnce(Result<T, String>) -> AppEvent + Send + 'static,
) where
    T: Send + 'static,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        std::mem::drop(handle.spawn(async move {
            app_event_tx.send(completion_event(future.await));
        }));
    } else {
        let _ = std::thread::spawn(move || {
            let result = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime.block_on(future),
                Err(err) => {
                    tracing::warn!(error = %err, "failed to start pet load runtime");
                    Err(format!("failed to start pet load runtime: {err}"))
                }
            };
            app_event_tx.send(completion_event(result));
        });
    }
}

#[cfg(test)]
#[path = "pets_tests.rs"]
mod tests;
