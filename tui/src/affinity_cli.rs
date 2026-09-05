mod costs;
mod models;

use crossterm::event::KeyCode;
use estelle_client::AccountResponse;
use estelle_client::FleetSnapshot;
use ratatui::Frame;
use ratatui::layout::Rect;

pub(crate) use costs::CostLedger;
pub(crate) use models::ModelsScreen;

#[derive(Clone, Debug)]
pub(crate) enum Surface {
    Models(Box<ModelsScreen>),
    Costs,
}

impl Surface {
    #[allow(
        dead_code,
        reason = "the affinity MODELS surface has no key or command to reach it. Its only door was `ctrl+m`, which is carriage return in this binary and was removed rather than moved, because choosing its replacement is a founder ruling open on design-book screen 10. The code is kept, not deleted, so the ruling is one binding away"
    )]
    pub(crate) fn models_loading() -> Self {
        Self::Models(Box::new(ModelsScreen::loading()))
    }

    pub(crate) fn render(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        theme: crate::Theme,
        costs: &CostLedger,
        account: Option<&AccountResponse>,
        fleet: Option<&FleetSnapshot>,
    ) {
        match self {
            Self::Models(screen) => screen.render(frame, area, theme),
            Self::Costs => costs.render(frame, area, theme, account, fleet),
        }
    }

    pub(crate) fn models_mut(&mut self) -> Option<&mut ModelsScreen> {
        match self {
            Self::Models(screen) => Some(screen.as_mut()),
            Self::Costs => None,
        }
    }

    pub(crate) fn is_costs(&self) -> bool {
        matches!(self, Self::Costs)
    }

    #[allow(
        dead_code,
        reason = "the affinity MODELS surface has no key or command to reach it. Its only door was `ctrl+m`, which is carriage return in this binary and was removed rather than moved, because choosing its replacement is a founder ruling open on design-book screen 10. The code is kept, not deleted, so the ruling is one binding away"
    )]
    pub(crate) fn is_models(&self) -> bool {
        matches!(self, Self::Models(_))
    }

    pub(crate) fn handle_models_key(&mut self, code: KeyCode, reverse: bool) -> bool {
        let Some(models) = self.models_mut() else {
            return false;
        };
        match code {
            KeyCode::Tab | KeyCode::Right => models.select_role(reverse),
            KeyCode::BackTab | KeyCode::Left => models.select_role(true),
            KeyCode::Down => models.select_option(false),
            KeyCode::Up => models.select_option(true),
            _ => return false,
        }
        true
    }
}
