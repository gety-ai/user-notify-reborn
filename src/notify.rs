use std::{collections::HashMap, fmt::Debug};

use async_trait::async_trait;

use crate::Error;

#[derive(Debug, Default, Clone)]
pub struct NotifyBuilder {
    pub(crate) body: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) subtitle: Option<String>,
    pub(crate) thread_id: Option<String>,
    pub(crate) category_id: Option<String>,
    pub(crate) user_metadata: Option<HashMap<String, String>>,
    pub(crate) sound: Option<String>,
}

impl NotifyBuilder {
    pub fn new() -> Self {
        NotifyBuilder::default()
    }

    /// Set main content of notification
    ///
    /// Windows: subtitle & content share 4 lines
    /// macOS: subtitle & content share 4 lines, but subtitle only 1 line
    pub fn body(mut self, body: &str) -> Self {
        self.body = Some(body.to_owned());
        self
    }

    /// Set primary description of notification
    ///
    /// Windows: 2 lines allowed
    /// macOS: 1 line allowed
    pub fn title(mut self, title: &str) -> Self {
        self.title = Some(title.to_owned());
        self
    }

    /// Set secondary description of notification
    ///
    /// Windows: subtitle & content share 4 lines
    /// macOS: 1 line allowed
    pub fn subtitle(mut self, subtitle: &str) -> Self {
        self.subtitle = Some(subtitle.to_owned());
        self
    }

    /// Set notification sound
    ///
    /// Windows: Not supported
    /// macOS: [UNNotificationContent/sound](https://developer.apple.com/documentation/usernotifications/unnotificationcontent/sound)
    ///   - Use "default" for default system sound
    ///   - Use filename without extension for custom sounds (must be in app bundle)
    pub fn sound(mut self, sound: &str) -> Self {
        self.sound = Some(sound.to_owned());
        self
    }

    /// Set thread id for grouping related notifications
    ///
    /// Windows: Not supported
    /// macOS: [UNNotificationContent/threadIdentifier](https://developer.apple.com/documentation/usernotifications/unnotificationcontent/threadidentifier)
    pub fn set_thread_id(mut self, thread_id: &str) -> Self {
        self.thread_id = Some(thread_id.to_owned());
        self
    }

    /// Set notification category
    pub fn set_category_id(mut self, category_id: &str) -> Self {
        self.category_id = Some(category_id.to_owned());
        self
    }

    /// Set metadata for a notification
    pub fn set_user_metadata(mut self, user_metadata: HashMap<String, String>) -> Self {
        self.user_metadata = Some(user_metadata);
        self
    }
}

/// Handle to a sent notification
pub trait NotifyHandleExt
where
    Self: Send + Sync + Debug,
{
    /// Close the notification
    fn close(&self) -> Result<(), Error>;

    /// Get the notification ID
    fn get_id(&self) -> String;
}

#[async_trait]
pub trait NotifyManagerExt
where
    Self: Send + Sync + Debug,
{
    type NotifyHandle: NotifyHandleExt;

    /// Get notification permission state
    async fn get_notification_permission_state(&self) -> Result<bool, crate::Error>;

    /// Ask for notification permission for the first time
    async fn first_time_ask_for_notification_permission(&self) -> Result<bool, Error>;

    /// Register notification handler and categories
    fn register(
        &self,
        handler_callback: Box<dyn Fn(crate::NotifyResponse) + Send + Sync + 'static>,
        categories: Vec<NotifyCategory>,
    ) -> Result<(), Error>;

    /// Remove all delivered notifications
    fn remove_all_delivered_notifications(&self) -> Result<(), Error>;

    /// Remove specific delivered notifications by their id
    fn remove_delivered_notifications(&self, ids: Vec<&str>) -> Result<(), Error>;

    /// Get all delivered notifications that are still active
    async fn get_active_notifications(&self) -> Result<Vec<Self::NotifyHandle>, Error>;

    /// Send notification and return notification handle
    async fn send(&self, builder: NotifyBuilder) -> Result<Self::NotifyHandle, Error>;
}

#[derive(Debug, Clone)]
pub struct NotifyResponse {
    /// ID of the notification that was assigned by the system
    pub notification_id: String,
    pub action: NotifyResponseAction,
    /// The text that the user typed in as response
    pub user_input: Option<String>,
    pub user_metadata: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub enum NotifyResponseAction {
    /// When user clicks on the notification
    Default,
    /// When user closes the notification
    Dismiss,
    /// The identifier string of the action that the user selected
    Other(String),
}

#[derive(Debug, Clone)]
pub struct NotifyCategory {
    /// ID of the category by which it is referenced on notifications
    pub identifier: String,
    /// The actions to display when the system delivers notifications of this type
    pub actions: Vec<NotifyCategoryAction>,
}

#[derive(Debug, Clone)]
pub enum NotifyCategoryAction {
    Action {
        identifier: String,
        title: String,
    },
    TextInputAction {
        identifier: String,
        title: String,
        input_button_title: String,
        input_placeholder: String,
    },
}
