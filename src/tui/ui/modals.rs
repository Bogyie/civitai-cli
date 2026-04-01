use ratatui::Frame;

use crate::tui::app::App;

pub(super) fn draw_active_modals(f: &mut Frame, app: &mut App) {
    if app.show_status_modal {
        super::draw_status_modal(f, app);
    }

    if app.show_help_modal {
        super::draw_help_modal(f, app);
    }

    if app.show_image_prompt_modal {
        super::draw_image_prompt_modal(f, app);
    }

    if app.show_image_model_detail_modal {
        super::draw_image_model_detail_modal(f, app);
    }

    if app.show_bookmark_confirm_modal {
        super::draw_bookmark_confirm_modal(f, app);
    }

    if app.show_exit_confirm_modal {
        super::draw_exit_confirm_modal(f, app);
    }

    if app.show_resume_download_modal {
        super::draw_resume_download_modal(f, app);
    }
}
