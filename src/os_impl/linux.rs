//! Linux stub implementation for user-notify-reborn
//!
//! This module provides stub implementations for Linux that return
//! `NotSupported` errors. Linux notification support is not implemented.

use std::sync::Arc;

use async_trait::async_trait;

use crate::{Error, NotifyBuilder, NotifyCategory, NotifyHandleExt, NotifyManagerExt};

/// A stub handle for notifications on Linux.
#[derive(Debug)]
pub struct NotifyHandle {
    id: String,
}

impl NotifyHandle {
    fn new(id: String) -> Self {
        Self { id }
    }
}

impl NotifyHandleExt for NotifyHandle {
    fn close(&self) -> Result<(), Error> {
        Err(Error::NotSupported)
    }

    fn get_id(&self) -> String {
        self.id.clone()
    }
}

/// Linux stub notification manager.
#[derive(Debug, Clone)]
pub struct NotifyManager {
    _inner: Arc<()>,
}

impl NotifyManager {
    /// Creates a new notification manager instance (stub).
    #[allow(clippy::new_without_default)]
    pub fn new_() -> Self {
        Self {
            _inner: Arc::new(()),
        }
    }

    /// Attempts to create a new notification manager (stub).
    pub fn try_new(_bundle_id: &str, _category_identifier: Option<&str>) -> Result<Self, Error> {
        Ok(Self::new_())
    }
}

#[async_trait]
impl NotifyManagerExt for NotifyManager {
    type NotifyHandle = NotifyHandle;

    async fn get_notification_permission_state(&self) -> Result<bool, Error> {
        Err(Error::NotSupported)
    }

    async fn first_time_ask_for_notification_permission(&self) -> Result<bool, Error> {
        Err(Error::NotSupported)
    }

    fn register(
        &self,
        _handler_callback: Box<dyn Fn(crate::NotifyResponse) + Send + Sync + 'static>,
        _categories: Vec<NotifyCategory>,
    ) -> Result<(), Error> {
        Err(Error::NotSupported)
    }

    fn remove_all_delivered_notifications(&self) -> Result<(), Error> {
        Err(Error::NotSupported)
    }

    fn remove_delivered_notifications(&self, _ids: Vec<&str>) -> Result<(), Error> {
        Err(Error::NotSupported)
    }

    async fn get_active_notifications(&self) -> Result<Vec<Self::NotifyHandle>, Error> {
        Err(Error::NotSupported)
    }

    async fn send(&self, _builder: NotifyBuilder) -> Result<Self::NotifyHandle, Error> {
        Err(Error::NotSupported)
    }
}
