mod error;
mod notify;
mod os_impl;

use std::sync::Arc;

pub use error::Error;
pub use notify::*;
pub use os_impl::*;

/// Default factory for creating notification managers
#[derive(Debug, Default)]
pub struct NotifyManagerFactory;

impl NotifyManagerFactory {
    /// Create a notification manager for the current platform
    pub fn new(
        app_id: String,
        notification_protocol: Option<String>,
    ) -> Result<Arc<dyn NotifyManager>, Error> {
        #[cfg(target_os = "windows")]
        {
            use ::windows::core::HSTRING;

            match ::windows::UI::Notifications::ToastNotificationManager::CreateToastNotifierWithId(
                &HSTRING::from(&app_id),
            ) {
                Ok(_tf) => Ok(Arc::new(
                    os_impl::windows::NotifyManagerWindows::new(
                        app_id.clone(),
                        notification_protocol,
                    ),
                )),
                Err(err) => Err(Error::Other(format!(
                    "failed to get toast notifier for {app_id}: {err:?}"
                ))),
            }
        }

        // #[cfg(target_os = "macos")]
        // {
        //     use objc2_foundation::NSBundle;

        //     if unsafe { NSBundle::mainBundle().bundleIdentifier().is_none() } {
        //         return Err(Error::Other(format!(
        //             "bundle id is not set, this is required to send notifications"
        //         )));
        //     }

        //     Ok(Arc::new(
        //         os_impl::macos::NotifyManagerMacOS::new(),
        //     ))
        // }

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            Err(Error::Other(format!("unsupported platform")))
        }
    }
}
