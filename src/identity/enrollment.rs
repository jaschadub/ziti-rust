//! Enrollment process implementation
//!
//! Handles JWT-based enrollment and identity creation.

use super::IdentityConfig;
use crate::error::ZitiResult;

/// Enrollment manager for JWT-based enrollment
pub struct EnrollmentManager;

impl EnrollmentManager {
    pub fn new() -> Self {
        Self
    }

    pub async fn enroll_with_jwt(&self, _jwt_token: &str) -> ZitiResult<IdentityConfig> {
        todo!("Implement enroll_with_jwt")
    }
}

impl Default for EnrollmentManager {
    fn default() -> Self {
        Self::new()
    }
}
