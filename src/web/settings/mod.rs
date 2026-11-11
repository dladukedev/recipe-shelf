use super::*;
pub(super) mod basic;
pub(super) mod folders;
mod frame;
pub(super) mod templates;
pub(super) mod users;
use frame::{SettingsSection, settings_frame};

#[derive(Deserialize)]
pub(in crate::web) struct IdForm {
    csrf: String,
    id: i64,
}
