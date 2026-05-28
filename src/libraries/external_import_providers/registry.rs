//! Registry of external import providers.
//!
//! WHAT: holds all providers registered by a builder and supports lookup by file extension.
//! WHY: Stage 0 and header import preparation need to discover which provider (if any)
//!      handles a given external file import.

use super::provider::ExternalImportProvider;
use std::sync::Arc;

/// Cloneable registry of `ExternalImportProvider` implementations.
///
/// WHAT: stores providers as `Arc<dyn ExternalImportProvider>` so the registry and the
///       `LibrarySet` that owns it remain cheap to clone.
/// WHY: `LibrarySet` derives `Clone`; the provider registry must preserve that ergonomics.
#[derive(Clone)]
pub struct ExternalImportProviderRegistry {
    providers: Vec<Arc<dyn ExternalImportProvider>>,
}

impl std::fmt::Debug for ExternalImportProviderRegistry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ExternalImportProviderRegistry")
            .field("provider_count", &self.providers.len())
            .finish()
    }
}

impl Default for ExternalImportProviderRegistry {
    fn default() -> Self {
        Self::empty()
    }
}

impl ExternalImportProviderRegistry {
    /// Creates an empty registry with no providers.
    pub fn empty() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    /// Registers a provider.
    pub fn register(&mut self, provider: Arc<dyn ExternalImportProvider>) {
        self.providers.push(provider);
    }

    /// Iterates over all registered providers.
    pub fn iter(&self) -> impl Iterator<Item = &Arc<dyn ExternalImportProvider>> {
        self.providers.iter()
    }

    /// Finds the first provider that supports the given file extension.
    pub fn find_by_extension(&self, extension: &str) -> Option<&Arc<dyn ExternalImportProvider>> {
        self.providers.iter().find(|provider| {
            provider
                .supported_extensions()
                .iter()
                .any(|ext| ext.as_str() == extension)
        })
    }

    /// Returns true if any registered provider supports the given file extension.
    pub fn supports_extension(&self, extension: &str) -> bool {
        self.find_by_extension(extension).is_some()
    }

    /// Returns the number of registered providers.
    pub fn len(&self) -> usize {
        self.providers.len()
    }

    /// Returns true if no providers are registered.
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }
}
