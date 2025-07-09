pub mod service;
pub mod types;
#[cfg(test)]
mod tests;

pub use service::LicenseService;
pub use types::UserLicense;