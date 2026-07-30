use std::sync::Arc;

use super::Provider;

pub(super) struct DispatchLease {
    pub(super) provider: Arc<dyn Provider>,
}

impl DispatchLease {
    pub(super) fn acquire(provider: Arc<dyn Provider>) -> Option<Self> {
        provider.acquire_dispatch().then_some(Self { provider })
    }
}

impl Drop for DispatchLease {
    fn drop(&mut self) {
        self.provider.release_dispatch();
    }
}
