use crate::{
    Error, NotifyBuilder, NotifyCategory, NotifyHandleExt, NotifyManagerExt, NotifyResponseAction,
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};
use windows::core::{IInspectable, Interface, HSTRING};
use windows::Foundation::Collections::StringMap;
use windows::Foundation::TypedEventHandler;
use windows::UI::Notifications::{
    NotificationData, ToastActivatedEventArgs, ToastDismissalReason, ToastDismissedEventArgs,
    ToastNotifier,
};
use windows::{UI::Notifications::ToastNotification, UI::Notifications::ToastNotificationManager};
use windows_collections::IVectorView;

mod builder;

/// Windows-specific notification handle implementation.
///
/// This handle represents an active notification in the Windows notification system.
/// It provides methods to interact with individual notifications.
///
/// # References
/// - [Windows Toast Notifications](https://docs.microsoft.com/en-us/windows/apps/design/shell/tiles-and-notifications/adaptive-interactive-toasts)
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct NotifyHandle {
    id: String,
    user_metadata: HashMap<String, String>,
}

impl NotifyHandleExt for NotifyHandle {
    fn close(&self) -> Result<(), crate::Error> {
        log::info!("Windows: Closing notification {}", self.id);
        Ok(())
    }

    fn get_id(&self) -> String {
        self.id.clone()
    }
}

/// Windows notification manager implementation using Windows Runtime APIs.
///
/// This manager handles toast notifications on Windows 10/11 systems using the
/// Windows Runtime (WinRT) APIs through the `windows` crate bindings.
///
/// # References
/// - [Toast Notification Manager](https://docs.microsoft.com/en-us/uwp/api/windows.ui.notifications.toastnotificationmanager)
/// - [Desktop Bridge notifications](https://docs.microsoft.com/en-us/windows/apps/design/shell/tiles-and-notifications/send-local-toast-desktop)
/// - [Windows Runtime APIs in Rust](https://docs.rs/windows/latest/windows/)
pub struct NotifyManager {
    #[allow(clippy::type_complexity)]
    handler_callback: Arc<OnceLock<Box<dyn Fn(crate::NotifyResponse) + Send + Sync + 'static>>>,
    app_id: String,
    notification_protocol: Option<String>,
    categories: Arc<RwLock<HashMap<String, NotifyCategory>>>,
}

impl std::fmt::Debug for NotifyManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotifyManagerWindows")
            .field(
                "handler_callback",
                match &self.handler_callback.get() {
                    Some(_) => &"handler",
                    None => &"no handler",
                },
            )
            .finish()
    }
}

const MESSAGE_GROUP: &str = "msg-group";
const USER_INFO_JSON_KEY: &str = "UserInfoJson";

impl NotifyManager {
    fn new_(app_id: String, notification_protocol: Option<String>) -> Self {
        Self {
            handler_callback: Arc::new(OnceLock::new()),
            app_id,
            notification_protocol,
            categories: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn try_new(app_id: &str, notification_protocol: Option<&str>) -> Result<Self, Error> {
        use windows::core::HSTRING;
        match ::windows::UI::Notifications::ToastNotificationManager::CreateToastNotifierWithId(
            &HSTRING::from(app_id),
        ) {
            Ok(_tf) => Ok(Self::new_(
                app_id.to_string(),
                notification_protocol.map(|s| s.to_string()),
            )),
            Err(err) => Err(Error::Other(format!(
                "failed to get toast notifier for {app_id}: {err:?}"
            ))),
        }
    }

    /// Creates a ToastNotifier for the configured app ID.
    ///
    /// # References
    /// - [ToastNotifier Class](https://docs.microsoft.com/en-us/uwp/api/windows.ui.notifications.toastnotifier)
    fn get_toast_notifier(&self) -> Result<ToastNotifier, Error> {
        let toast_notifier =
            ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(&self.app_id));
        Ok(toast_notifier?)
    }

    /// Retrieves notification history for the app.
    ///
    /// # References
    /// - [Notification History](https://docs.microsoft.com/en-us/uwp/api/windows.ui.notifications.toastnotificationhistory)
    fn get_history(&self) -> Result<IVectorView<ToastNotification>, Error> {
        let history =
            ToastNotificationManager::History()?.GetHistoryWithId(&HSTRING::from(&self.app_id));
        Ok(history?)
    }

    fn user_info_from_toast(toast: &ToastNotification) -> Result<HashMap<String, String>, Error> {
        let user_info_string = toast
            .Data()?
            .Values()?
            .Lookup(&HSTRING::from(USER_INFO_JSON_KEY.to_string()))?;

        let user_info: HashMap<String, String> =
            serde_json::from_str(&quick_xml::escape::unescape(&user_info_string.to_string())?)
                .map_err(Error::FailedToParseUserInfo)?;
        Ok(user_info)
    }

    /// Generate the notification ID for a new notification
    fn generate_notification_id() -> String {
        uuid::Uuid::new_v4().to_string()[..16].to_owned()
    }

    /// Serialize user metadata to JSON string
    fn serialize_user_metadata(user_metadata: &Option<HashMap<String, String>>) -> String {
        user_metadata
            .as_ref()
            .and_then(|user_metadata| match serde_json::to_string(user_metadata) {
                Ok(user_metadata_string) => Some(user_metadata_string),
                Err(err) => {
                    log::error!("failed to serialize user_metadata: ({user_metadata:?}) {err:?}");
                    None
                }
            })
            .unwrap_or("{}".to_string())
    }

    /// Create and configure the Windows Toast notification
    fn create_toast_notification(
        &self,
        builder: &NotifyBuilder,
        notification_id: &str,
        user_metadata_string: &str,
    ) -> Result<ToastNotification, Error> {
        let toast_xml = builder::build_toast_xml(
            builder.clone(),
            notification_id,
            self.notification_protocol.as_deref(),
            |category_id| self.generate_actions_xml(category_id),
        )?;

        let toast = ToastNotification::CreateToastNotification(&toast_xml)?;

        self.configure_toast_notification(&toast, notification_id, user_metadata_string)?;

        Ok(toast)
    }

    /// Configure toast notification properties (tag, group, data)
    fn configure_toast_notification(
        &self,
        toast: &ToastNotification,
        notification_id: &str,
        user_metadata_string: &str,
    ) -> Result<(), Error> {
        toast.SetTag(&HSTRING::from(notification_id))?;
        toast.SetGroup(&HSTRING::from(MESSAGE_GROUP))?;

        let user_info_map = StringMap::new()?;
        user_info_map.Insert(
            &HSTRING::from(USER_INFO_JSON_KEY),
            &HSTRING::from(user_metadata_string),
        )?;

        toast.SetData(&NotificationData::CreateNotificationDataWithValues(
            &user_info_map,
        )?)?;

        Ok(())
    }

    /// Create a notification handle from the builder and ID
    fn create_notification_handle(
        builder: &NotifyBuilder,
        notification_id: String,
    ) -> NotifyHandle {
        NotifyHandle {
            id: notification_id,
            user_metadata: builder.user_metadata.clone().unwrap_or_default(),
        }
    }

    /// Generate XML for standard action buttons
    fn generate_action_xml(identifier: &str, title: &str) -> String {
        let escaped_identifier = quick_xml::escape::escape(identifier);
        let escaped_title = quick_xml::escape::escape(title);
        format!(
            r#"<action content="{escaped_title}" arguments="{escaped_identifier}" activationType="foreground" />"#
        )
    }

    /// Generate XML for text input actions
    fn generate_text_input_action_xml(
        identifier: &str,
        input_button_title: &str,
        input_placeholder: &str,
    ) -> String {
        let escaped_identifier = quick_xml::escape::escape(identifier);
        let escaped_button_title = quick_xml::escape::escape(input_button_title);
        let escaped_placeholder = quick_xml::escape::escape(input_placeholder);

        format!(
            r#"<input id="textBox" type="text" placeHolderContent="{escaped_placeholder}" /><action content="{escaped_button_title}" arguments="{escaped_identifier}" hint-inputId="textBox" activationType="foreground" />"#
        )
    }

    /// Generates action XML elements for notification categories.
    ///
    /// Creates interactive buttons and input fields for toast notifications based on
    /// the registered notification categories.
    ///
    /// # References
    /// - [Toast Actions](https://docs.microsoft.com/en-us/windows/apps/design/shell/tiles-and-notifications/adaptive-interactive-toasts#actions)
    /// - [Toast Inputs](https://docs.microsoft.com/en-us/windows/apps/design/shell/tiles-and-notifications/adaptive-interactive-toasts#inputs)
    fn generate_actions_xml(&self, category_id: &str) -> Result<String, Error> {
        let categories = self.categories.read().map_err(|_| Error::SettingHandler)?;

        if let Some(category) = categories.get(category_id) {
            if category.actions.is_empty() {
                return Ok(String::new());
            }

            let mut actions_xml = String::from("<actions>");

            for action in &category.actions {
                let action_xml = match action {
                    crate::NotifyCategoryAction::Action { identifier, title } => {
                        Self::generate_action_xml(identifier, title)
                    }
                    crate::NotifyCategoryAction::TextInputAction {
                        identifier,
                        title: _,
                        input_button_title,
                        input_placeholder,
                    } => Self::generate_text_input_action_xml(
                        identifier,
                        input_button_title,
                        input_placeholder,
                    ),
                };
                actions_xml.push_str(&action_xml);
            }

            actions_xml.push_str("</actions>");
            Ok(actions_xml)
        } else {
            log::warn!("Category '{category_id}' not found in registered categories");
            Ok(String::new())
        }
    }

    /// Extract activated action from toast event arguments
    fn get_activated_action(insp: &Option<IInspectable>) -> Option<String> {
        insp.as_ref().and_then(|insp| {
            insp.cast::<ToastActivatedEventArgs>()
                .and_then(|args| args.Arguments())
                .ok()
                .and_then(|arguments| {
                    if !arguments.is_empty() {
                        Some(arguments.to_string())
                    } else {
                        None
                    }
                })
        })
    }

    /// Extract dismissal reason from toast event arguments
    fn get_dismissed_reason(
        args: &Option<ToastDismissedEventArgs>,
    ) -> Option<ToastDismissalReason> {
        args.as_ref().and_then(|args| args.Reason().ok())
    }

    /// Create activation event handler for toast notifications.
    ///
    /// Handles user interactions with toast notifications including button clicks
    /// and protocol activations.
    ///
    /// # References
    /// - [Toast Activated Event](https://docs.microsoft.com/en-us/uwp/api/windows.ui.notifications.toastnotification.activated)
    /// - [Handling activation](https://docs.microsoft.com/en-us/windows/apps/design/shell/tiles-and-notifications/send-local-toast-desktop#handling-activation)
    fn create_activation_handler(
        &self,
        notification_id: String,
        user_info: HashMap<String, String>,
    ) -> TypedEventHandler<ToastNotification, IInspectable> {
        let handler_callback = self.handler_callback.clone();
        let notification_protocol = self.notification_protocol.clone();
        TypedEventHandler::new(move |_, insp| {
            let action = Self::get_activated_action(&insp);
            if let Some(handler) = handler_callback.get() {
                let response_action = action
                    .map(|action_str| {
                        // If we have a notification protocol, decode as deeplink
                        if notification_protocol.is_some() {
                            builder::decode_deeplink(&action_str)
                                .map(|response| response.action)
                                .inspect_err(|err| {
                                    log::error!("failed to extract action from {action_str}: {err}")
                                })
                                .unwrap_or_else(|_| NotifyResponseAction::Other(action_str))
                        } else {
                            // Without notification protocol, treat as plain identifier
                            NotifyResponseAction::Other(action_str)
                        }
                    })
                    .unwrap_or(NotifyResponseAction::Default);

                handler(crate::NotifyResponse {
                    notification_id: notification_id.clone(),
                    action: response_action,
                    user_input: None,
                    user_metadata: user_info.clone(),
                })
            }
            Ok(())
        })
    }

    /// Create dismissal event handler for toast notifications.
    ///
    /// Handles notification dismissal events to track user interactions.
    ///
    /// # References
    /// - [Toast Dismissed Event](https://docs.microsoft.com/en-us/uwp/api/windows.ui.notifications.toastnotification.dismissed)
    /// - [Toast Dismissal Reasons](https://docs.microsoft.com/en-us/uwp/api/windows.ui.notifications.toastdismissalreason)
    fn create_dismissal_handler(
        &self,
        notification_id: String,
        user_info: HashMap<String, String>,
    ) -> TypedEventHandler<ToastNotification, ToastDismissedEventArgs> {
        let handler_callback = self.handler_callback.clone();
        TypedEventHandler::new(move |_, args| {
            let reason = Self::get_dismissed_reason(&args);
            match reason {
                Some(ToastDismissalReason::UserCanceled) => {
                    if let Some(handler) = handler_callback.get() {
                        handler(crate::NotifyResponse {
                            notification_id: notification_id.clone(),
                            action: NotifyResponseAction::Dismiss,
                            user_input: None,
                            user_metadata: user_info.clone(),
                        })
                    }
                }
                _ => log::debug!("dismissed toast: {reason:?}"),
            }
            Ok(())
        })
    }

    fn register_event_listeners(&self, toast: &ToastNotification) -> Result<(), Error> {
        let notification_id = toast.Tag()?.to_string();
        let user_info = Self::user_info_from_toast(toast).unwrap_or_default();

        let activation_handler =
            self.create_activation_handler(notification_id.clone(), user_info.clone());
        let dismissal_handler = self.create_dismissal_handler(notification_id, user_info);

        toast.Activated(&activation_handler)?;
        toast.Dismissed(&dismissal_handler)?;
        Ok(())
    }

    /// Store notification categories for later use
    fn store_categories(&self, categories: Vec<NotifyCategory>) -> Result<(), Error> {
        let mut stored_categories = self.categories.write().map_err(|_| Error::SettingHandler)?;
        stored_categories.clear();
        for category in categories {
            stored_categories.insert(category.identifier.clone(), category);
        }
        Ok(())
    }

    /// Register event listeners for historical notifications
    fn register_historical_notifications(&self) -> Result<(), Error> {
        let history = self.get_history()?;
        for toast in history.into_iter() {
            if let Err(err) = self.register_event_listeners(&toast) {
                log::error!(
                    "failed to register event listener to toast from previous session {err:?}"
                );
            }
        }
        Ok(())
    }

    /// Clear all notifications with fallback to app-specific clearing
    fn clear_all_notifications(&self) -> Result<(), Error> {
        match ToastNotificationManager::History()?.Clear() {
            Ok(_) => Ok(()),
            Err(err) => {
                log::warn!("Failed to clear notification history: {err:?}");
                self.clear_notifications_by_app_id()?;
                Ok(()) // Don't fail the operation for cleanup issues
            }
        }
    }

    /// Clear notifications by app ID as fallback
    fn clear_notifications_by_app_id(&self) -> Result<(), Error> {
        if let Ok(manager) = ToastNotificationManager::History() {
            if let Err(clear_err) = manager.ClearWithId(&HSTRING::from(self.app_id.clone())) {
                log::warn!("Failed to clear notifications for app ID: {clear_err:?}");
            }
        }
        Ok(())
    }

    /// Remove a single notification by ID
    fn remove_notification_by_id(&self, id: &str) {
        if let Ok(manager) = ToastNotificationManager::History() {
            if let Err(err) = manager.RemoveGroupedTagWithId(
                &HSTRING::from(id.to_owned()),
                &HSTRING::from(MESSAGE_GROUP.to_owned()),
                &HSTRING::from(self.app_id.clone()),
            ) {
                log::error!("failed to remove toast notification with tag {id}: {err:?}");
            }
        }
    }
}

#[async_trait]
impl NotifyManagerExt for NotifyManager {
    type NotifyHandle = NotifyHandle;

    async fn get_notification_permission_state(&self) -> Result<bool, crate::Error> {
        Ok(true)
    }

    async fn first_time_ask_for_notification_permission(&self) -> Result<bool, crate::Error> {
        Ok(true)
    }

    fn register(
        &self,
        handler_callback: Box<dyn Fn(crate::NotifyResponse) + Send + Sync + 'static>,
        categories: Vec<crate::NotifyCategory>,
    ) -> Result<(), crate::Error> {
        log::info!(
            "Windows: Registering notification handler with {} categories",
            categories.len()
        );

        self.handler_callback
            .set(handler_callback)
            .map_err(|_| Error::SettingHandler)?;

        self.store_categories(categories)?;
        self.register_historical_notifications()?;

        Ok(())
    }

    fn remove_all_delivered_notifications(&self) -> Result<(), crate::Error> {
        self.clear_all_notifications()
    }

    fn remove_delivered_notifications(&self, ids: Vec<&str>) -> Result<(), crate::Error> {
        for id in ids {
            self.remove_notification_by_id(id);
        }
        Ok(())
    }

    async fn get_active_notifications(&self) -> Result<Vec<NotifyHandle>, crate::Error> {
        let history = self.get_history()?;

        let mut handles: Vec<NotifyHandle> = Vec::new();

        for toast in history.into_iter() {
            let user_metadata: HashMap<String, String> =
                Self::user_info_from_toast(&toast).unwrap_or_default();

            handles.push(NotifyHandle {
                id: toast.Tag()?.to_string(),
                user_metadata,
            });
        }

        log::debug!("Windows: Found {} active notifications", handles.len());

        Ok(handles)
    }

    async fn send(&self, builder: NotifyBuilder) -> Result<NotifyHandle, crate::Error> {
        log::info!("Windows: Sending notification");

        let notification_id = Self::generate_notification_id();
        let user_metadata_string = Self::serialize_user_metadata(&builder.user_metadata);

        let toast =
            self.create_toast_notification(&builder, &notification_id, &user_metadata_string)?;

        self.register_event_listeners(&toast)?;
        self.get_toast_notifier()?.Show(&toast)?;

        let handle = Self::create_notification_handle(&builder, notification_id);
        Ok(handle)
    }
}
