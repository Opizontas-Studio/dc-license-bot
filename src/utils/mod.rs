mod children;
mod embed;
mod editor_core;
mod license_editor;

pub use children::get_all_children_channels;
pub use embed::LicenseEmbedBuilder;
pub use editor_core::{LicenseEditState, EditorCore, UIProvider};
pub use license_editor::present_license_editing_panel;
