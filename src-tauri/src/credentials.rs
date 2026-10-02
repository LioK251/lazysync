use crate::models::*;
use zeroize::Zeroizing;
pub trait CredentialStore: Send + Sync {
    fn get(&self) -> Result<Zeroizing<String>>;
    fn set(&self, token: &str) -> Result<()>;
}
pub struct NativeVault;
impl NativeVault {
    fn entry() -> Result<keyring::Entry> {
        keyring::Entry::new("lazysync", "github.com").map_err(|_| {
            AppError::new(
                "vault",
                "Credential vault is unavailable.",
                "Unlock your operating system credential vault.",
            )
        })
    }
}
impl CredentialStore for NativeVault {
    fn get(&self) -> Result<Zeroizing<String>> {
        Self::entry()?
            .get_password()
            .map(Zeroizing::new)
            .map_err(|_| {
                AppError::new(
                    "authentication",
                    "Connect your GitHub account.",
                    "Enter a personal access token in Settings.",
                )
            })
    }
    fn set(&self, token: &str) -> Result<()> {
        Self::entry()?.set_password(token).map_err(|_| {
            AppError::new(
                "vault",
                "Could not save token in the native vault.",
                "Unlock your credential vault and try again.",
            )
        })
    }
}
