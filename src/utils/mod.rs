mod children;
mod editor_core;
mod embed;
mod license_editor;

pub use children::get_all_children_channels;
pub use editor_core::{EditorCore, LicenseEditState, UIProvider};
pub use embed::LicenseEmbedBuilder;
pub use license_editor::present_license_editing_panel;
