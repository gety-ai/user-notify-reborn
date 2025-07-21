mod builder;
mod delegate;

use crate::{
    Error, NotifyBuilder, NotifyCategory, NotifyHandleExt, NotifyManagerExt, NotifyResponse,
};
use async_trait::async_trait;
use builder::build_and_send;
use delegate::NotificationDelegate;
use objc2::{
    rc::Retained,
    runtime::{AnyObject, Bool, ProtocolObject},
    MainThreadMarker, Message,
};
use objc2_foundation::{NSArray, NSBundle, NSDictionary, NSError, NSSet, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNAuthorizationStatus, UNNotification, UNNotificationAction,
    UNNotificationActionOptions, UNNotificationCategory, UNNotificationCategoryOptions,
    UNNotificationRequest, UNNotificationSettings, UNTextInputNotificationAction,
    UNUserNotificationCenter, UNUserNotificationCenterDelegate,
};
use send_wrapper::SendWrapper;
use std::{
    cell::{OnceCell, RefCell},
    collections::HashMap,
    ops::Deref,
    ptr::NonNull,
    sync::Arc,
    thread,
};

// ============================================================================
// Constants and Type Aliases
// ============================================================================

/// Maximum number of notifications that can be queued in the response channel
const NOTIFICATION_RESPONSE_CHANNEL_SIZE: usize = 10;

/// Type alias for the delegate reference stored in the manager
type DelegateReference =
    SendWrapper<OnceCell<Retained<ProtocolObject<dyn UNUserNotificationCenterDelegate>>>>;

/// Type alias for the listener thread handle
type ListenerHandle = SendWrapper<OnceCell<thread::JoinHandle<()>>>;

// ============================================================================
// NotifyHandle - Individual Notification Handle
// ============================================================================

/// A handle for a specific notification on macOS platform.
///
/// This struct represents a single notification that has been posted to the
/// system and provides methods to interact with it.
///
/// # References
/// - [UNUserNotificationCenter](https://developer.apple.com/documentation/usernotifications/unusernotificationcenter)
/// - [UNNotification](https://developer.apple.com/documentation/usernotifications/unnotification)
#[derive(Debug)]
#[allow(dead_code)]
pub struct NotifyHandle {
    /// Unique identifier for the notification
    ///
    /// This corresponds to the `identifier` property of `UNNotificationRequest`
    id: String,
    user_info: HashMap<String, String>,
}

impl NotifyHandle {
    /// Creates a new notification handle
    ///
    /// # Arguments
    /// * `id` - Unique identifier for the notification
    /// * `user_data` - User-defined metadata
    ///
    /// # Returns
    /// A new `NotifyHandle` instance
    pub(super) fn new(id: String, user_data: HashMap<String, String>) -> Self {
        Self {
            id,
            user_info: user_data,
        }
    }

    /// Validates that we're running on the main thread
    ///
    /// # Returns
    /// `MainThreadMarker` if on main thread, `Error::NotMainThread` otherwise
    ///
    /// # References
    /// - [Main Thread Checker](https://developer.apple.com/documentation/xcode/main-thread-checker)
    fn ensure_main_thread() -> Result<MainThreadMarker, Error> {
        MainThreadMarker::new().ok_or(Error::NotMainThread)
    }

    /// Validates that the application has a valid bundle identifier
    ///
    /// # Returns
    /// `Ok(())` if bundle ID exists, `Error::NoBundleId` otherwise
    ///
    /// # References
    /// - [NSBundle.bundleIdentifier](https://developer.apple.com/documentation/foundation/nsbundle/1418023-bundleidentifier)
    fn ensure_bundle_id() -> Result<(), Error> {
        unsafe {
            NSBundle::mainBundle()
                .bundleIdentifier()
                .ok_or(Error::NoBundleId)
                .map(|_| ())
        }
    }

    /// Removes a single notification from the notification center
    ///
    /// # Arguments
    /// * `notification_id` - The ID of the notification to remove
    ///
    /// # References
    /// - [removeDeliveredNotificationsWithIdentifiers](https://developer.apple.com/documentation/usernotifications/unusernotificationcenter/1649500-removedeliverednotificationswith)
    fn remove_notification_by_id(notification_id: &str) -> Result<(), Error> {
        Self::ensure_main_thread()?;
        Self::ensure_bundle_id()?;

        let id = NSString::from_str(notification_id);
        let array: Retained<NSArray<NSString>> = NSArray::from_retained_slice(&[id]);

        unsafe {
            UNUserNotificationCenter::currentNotificationCenter()
                .removeDeliveredNotificationsWithIdentifiers(&array);
        }

        Ok(())
    }
}

impl NotifyHandleExt for NotifyHandle {
    /// Closes (removes) this notification from the system
    ///
    /// # Errors
    /// - `Error::NotMainThread` if not called from the main thread
    /// - `Error::NoBundleId` if the app doesn't have a valid bundle identifier
    ///
    /// # References
    /// - [UNUserNotificationCenter.removeDeliveredNotificationsWithIdentifiers](https://developer.apple.com/documentation/usernotifications/unusernotificationcenter/1649500-removedeliverednotificationswith)
    fn close(&self) -> Result<(), Error> {
        Self::remove_notification_by_id(&self.id)
    }

    /// Returns the unique identifier of this notification
    ///
    /// # Returns
    /// A clone of the notification's unique identifier
    fn get_id(&self) -> String {
        self.id.clone()
    }
}

// ============================================================================
// NotifyManager - Notification Management System
// ============================================================================

/// Internal state for the macOS notification manager
///
/// This struct holds the core components needed for notification management,
/// including delegate references and thread handles.
#[derive(Debug)]
pub struct NotifyManagerInner {
    /// Reference to the notification delegate to prevent it from being dropped
    ///
    /// The delegate handles notification responses and must remain alive
    /// for the duration of the application's notification handling.
    delegate_reference: DelegateReference,

    /// Handle to the background thread that processes notification responses
    ///
    /// This thread runs the event loop that forwards notification responses
    /// to the user-provided callback function.
    listener_loop: ListenerHandle,

    /// The application's bundle identifier
    ///
    /// Required for all notification operations on macOS.
    /// Derived from `NSBundle.mainBundle.bundleIdentifier`.
    pub(crate) bundle_id: Option<String>,
}

/// macOS implementation of the notification manager
///
/// This manager handles all notification operations on macOS using the
/// UserNotifications framework introduced in macOS 10.14.
///
/// # Architecture
/// The manager uses a delegate pattern where:
/// 1. A `NotificationDelegate` handles system callbacks
/// 2. Responses are forwarded through a channel to a background thread
/// 3. The background thread calls user-provided handlers
///
/// # Thread Safety
/// The manager is designed to be thread-safe and can be cloned.
/// All operations that interact with the UserNotifications framework
/// are properly synchronized.
///
/// # References
/// - [UserNotifications Framework](https://developer.apple.com/documentation/usernotifications)
/// - [UNUserNotificationCenter](https://developer.apple.com/documentation/usernotifications/unusernotificationcenter)
#[derive(Debug, Clone)]
pub struct NotifyManager {
    /// Shared internal state
    pub(super) inner: Arc<NotifyManagerInner>,
}

impl NotifyManager {
    /// Creates a new notification manager instance
    ///
    /// # Returns
    /// A new `NotifyManager` with initialized internal state
    ///
    /// # Note
    /// The bundle identifier is retrieved during construction and cached.
    /// If no bundle identifier is available, notification operations will fail.
    #[allow(clippy::new_without_default)]
    pub fn new_() -> Self {
        Self {
            inner: Arc::new(NotifyManagerInner {
                delegate_reference: SendWrapper::new(OnceCell::new()),
                listener_loop: SendWrapper::new(OnceCell::new()),
                bundle_id: Self::get_bundle_identifier(),
            }),
        }
    }

    pub fn try_new(_bundle_id: &str, _category_identifier: Option<&str>) -> Result<Self, Error> {
        use objc2_foundation::NSBundle;
        if unsafe { NSBundle::mainBundle().bundleIdentifier().is_none() } {
            return Err(Error::NoBundleId);
        }

        Ok(Self::new_())
    }

    /// Retrieves the application's bundle identifier
    ///
    /// # Returns
    /// `Some(String)` if bundle ID exists, `None` otherwise
    ///
    /// # References
    /// - [NSBundle.bundleIdentifier](https://developer.apple.com/documentation/foundation/nsbundle/1418023-bundleidentifier)
    fn get_bundle_identifier() -> Option<String> {
        unsafe {
            NSBundle::mainBundle()
                .bundleIdentifier()
                .map(|ns_string| ns_string.to_string())
        }
    }

    /// Validates that the manager has a valid bundle identifier
    ///
    /// # Returns
    /// Reference to bundle ID if valid, `Error::NoBundleId` otherwise
    fn ensure_valid_bundle_id(&self) -> Result<&str, Error> {
        self.inner.bundle_id.as_deref().ok_or(Error::NoBundleId)
    }

    /// Creates a completion handler for notification requests
    ///
    /// # Arguments
    /// * `callback` - Function to call when the operation completes
    ///
    /// # Returns
    /// A block that can be passed to UserNotifications framework functions
    ///
    /// # References
    /// - [UNUserNotificationCenter.addNotificationRequest](https://developer.apple.com/documentation/usernotifications/unusernotificationcenter/1649508-addnotificationrequest)
    fn create_notification_completion_handler<F>(
        callback: F,
    ) -> block2::RcBlock<dyn Fn(*mut NSError)>
    where
        F: FnOnce(Result<(), Error>) + Send + 'static,
    {
        let cb = RefCell::new(Some(callback));

        block2::RcBlock::new(move |error: *mut NSError| {
            if let Some(cb) = cb.take() {
                let result = if error.is_null() {
                    Ok(())
                } else {
                    unsafe { Err((&*error).into()) }
                };
                cb(result);
            }
        })
    }

    /// Adds a notification request to the notification center
    ///
    /// # Arguments
    /// * `request` - The notification request to add
    /// * `callback` - Completion handler called when the operation finishes
    ///
    /// # References
    /// - [UNUserNotificationCenter.addNotificationRequest](https://developer.apple.com/documentation/usernotifications/unusernotificationcenter/1649508-addnotificationrequest)
    pub(super) fn add_notification<F>(&self, request: &UNNotificationRequest, callback: F)
    where
        F: FnOnce(Result<(), Error>) + Send + 'static,
    {
        let block = Self::create_notification_completion_handler(callback);

        unsafe {
            UNUserNotificationCenter::currentNotificationCenter()
                .addNotificationRequest_withCompletionHandler(request, Some(&block));
        }
    }

    /// Creates a completion handler for authorization requests
    ///
    /// # Arguments
    /// * `sender` - Channel sender to send the result
    ///
    /// # Returns
    /// A block that processes authorization responses
    fn create_authorization_handler(
        sender: tokio::sync::oneshot::Sender<Result<bool, Error>>,
    ) -> block2::RcBlock<dyn Fn(Bool, *mut NSError)> {
        let cb = RefCell::new(Some(sender));

        block2::RcBlock::new(move |authorized: Bool, error: *mut NSError| {
            if let Some(cb) = cb.take() {
                let result = if error.is_null() {
                    Ok(authorized.as_bool())
                } else {
                    let err = Error::from(unsafe { &*error });

                    match err {
                        Error::NSError {
                            code, description, ..
                        } if code == 1 && description.contains("allowed") => Ok(false),
                        _ => Err(err),
                    }
                };

                if cb.send(result).is_err() {
                    log::error!("The receiver dropped");
                }
            }
        })
    }

    /// Requests notification authorization from the user
    ///
    /// # Arguments
    /// * `sender` - Channel to send the authorization result
    ///
    /// # References
    /// - [UNUserNotificationCenter.requestAuthorizationWithOptions](https://developer.apple.com/documentation/usernotifications/unusernotificationcenter/1649527-requestauthorizationwithoptions)
    fn request_notification_authorization(
        sender: tokio::sync::oneshot::Sender<Result<bool, Error>>,
    ) {
        let block = Self::create_authorization_handler(sender);

        let mut options = UNAuthorizationOptions::empty();
        options.set(UNAuthorizationOptions::Alert, true);
        options.set(UNAuthorizationOptions::Sound, true);
        options.set(UNAuthorizationOptions::Badge, true);

        unsafe {
            UNUserNotificationCenter::currentNotificationCenter()
                .requestAuthorizationWithOptions_completionHandler(options, &block);
        }
    }

    /// Creates a completion handler for notification settings queries
    ///
    /// # Arguments
    /// * `sender` - Channel to send the authorization status
    ///
    /// # Returns
    /// A block that processes notification settings
    fn create_settings_handler(
        sender: tokio::sync::oneshot::Sender<bool>,
    ) -> block2::RcBlock<dyn Fn(NonNull<UNNotificationSettings>)> {
        let cb = RefCell::new(Some(sender));

        block2::RcBlock::new(move |settings: NonNull<UNNotificationSettings>| {
            if let Some(cb) = cb.take() {
                let auth_status = unsafe { settings.as_ref().authorizationStatus() };
                let authorized = Self::is_authorization_status_granted(auth_status);

                if cb.send(authorized).is_err() {
                    log::error!("The receiver dropped");
                }
            }
        })
    }

    /// Determines if an authorization status represents granted permission
    ///
    /// # Arguments
    /// * `status` - The authorization status to check
    ///
    /// # Returns
    /// `true` if notifications are authorized, `false` otherwise
    ///
    /// # References
    /// - [UNAuthorizationStatus](https://developer.apple.com/documentation/usernotifications/unauthorizationstatus)
    fn is_authorization_status_granted(status: UNAuthorizationStatus) -> bool {
        match status {
            UNAuthorizationStatus::Authorized
            | UNAuthorizationStatus::Provisional
            | UNAuthorizationStatus::Ephemeral => true,
            UNAuthorizationStatus::Denied | UNAuthorizationStatus::NotDetermined => false,
            _ => {
                log::error!("Unknown authorization status: {:?}", status);
                false
            }
        }
    }

    /// Creates a completion handler for retrieving active notifications
    ///
    /// # Arguments
    /// * `sender` - Channel to send the list of notification handles
    ///
    /// # Returns
    /// A block that processes the notification list
    fn create_notifications_handler(
        sender: tokio::sync::oneshot::Sender<Vec<NotifyHandle>>,
    ) -> block2::RcBlock<dyn Fn(NonNull<NSArray<UNNotification>>)> {
        let cb = RefCell::new(Some(sender));

        block2::RcBlock::new(move |notifications: NonNull<NSArray<UNNotification>>| {
            if let Some(cb) = cb.take() {
                let notifications: &NSArray<UNNotification> = unsafe { notifications.as_ref() };
                let handles = Self::convert_notifications_to_handles(notifications);

                if cb.send(handles).is_err() {
                    log::error!("The receiver dropped");
                }
            } else {
                log::error!("tx was already taken out");
            }
        })
    }

    /// Converts a native notification array to a vector of handles
    ///
    /// # Arguments
    /// * `notifications` - Array of native notifications
    ///
    /// # Returns
    /// Vector of notification handles
    fn convert_notifications_to_handles(
        notifications: &NSArray<UNNotification>,
    ) -> Vec<NotifyHandle> {
        let mut handles = Vec::with_capacity(notifications.count());

        for item in notifications {
            unsafe {
                let request = item.request();
                let id = request.identifier().to_string();
                let user_info = user_info_dictionary_to_hashmap(request.content().userInfo());
                handles.push(NotifyHandle::new(id, user_info));
            }
        }

        handles
    }

    /// Removes multiple notifications by their identifiers
    ///
    /// # Arguments
    /// * `ids` - Vector of notification identifiers to remove
    ///
    /// # References
    /// - [removeDeliveredNotificationsWithIdentifiers](https://developer.apple.com/documentation/usernotifications/unusernotificationcenter/1649500-removedeliverednotificationswith)
    fn remove_notifications_by_ids(&self, ids: Vec<&str>) -> Result<(), Error> {
        self.ensure_valid_bundle_id()?;

        let ns_ids: Vec<_> = ids.iter().map(|s| NSString::from_str(s)).collect();
        let array: Retained<NSArray<NSString>> = NSArray::from_retained_slice(ns_ids.as_slice());

        unsafe {
            UNUserNotificationCenter::currentNotificationCenter()
                .removeDeliveredNotificationsWithIdentifiers(&array);
        }

        Ok(())
    }
}

#[async_trait]
impl NotifyManagerExt for NotifyManager {
    type NotifyHandle = NotifyHandle;

    /// Checks the current notification permission state
    ///
    /// # Returns
    /// `true` if notifications are authorized, `false` otherwise
    ///
    /// # Errors
    /// - `Error::NoBundleId` if the app doesn't have a valid bundle identifier
    /// - Communication errors from the async channel
    ///
    /// # References
    /// - [UNUserNotificationCenter.getNotificationSettings](https://developer.apple.com/documentation/usernotifications/unusernotificationcenter/1649524-getnotificationsettings)
    async fn get_notification_permission_state(&self) -> Result<bool, Error> {
        self.ensure_valid_bundle_id()?;

        let (tx, rx) = tokio::sync::oneshot::channel::<bool>();

        {
            let block = Self::create_settings_handler(tx);
            unsafe {
                UNUserNotificationCenter::currentNotificationCenter()
                    .getNotificationSettingsWithCompletionHandler(&block);
            }
        }

        Ok(rx.await?)
    }

    /// Requests notification permission from the user for the first time
    ///
    /// This method should only be called when the app needs to request
    /// notification permissions. It will show a system dialog to the user.
    ///
    /// # Returns
    /// `true` if permission was granted, `false` if denied
    ///
    /// # Errors
    /// - `Error::NoBundleId` if the app doesn't have a valid bundle identifier
    /// - `Error::NSError` for system-level errors
    /// - Communication errors from async channels
    ///
    /// # References
    /// - [Asking Permission to Use Notifications](https://developer.apple.com/documentation/usernotifications/asking_permission_to_use_notifications)
    async fn first_time_ask_for_notification_permission(&self) -> Result<bool, Error> {
        self.ensure_valid_bundle_id()?;

        let (tx, rx) = tokio::sync::oneshot::channel::<Result<bool, Error>>();
        Self::request_notification_authorization(tx);

        Ok(rx.await??)
    }

    /// Registers notification categories and sets up the response handler
    ///
    /// This method must be called before sending notifications that use
    /// custom categories or actions. It sets up the delegate and starts
    /// the response processing thread.
    ///
    /// # Arguments
    /// * `handler_callback` - Function called when users interact with notifications
    /// * `categories` - List of notification categories to register
    ///
    /// # Errors
    /// - `Error::NotMainThread` if not called from the main thread
    /// - Panics if called multiple times (OnceCell constraint)
    ///
    /// # References
    /// - [UNUserNotificationCenter.setNotificationCategories](https://developer.apple.com/documentation/usernotifications/unusernotificationcenter/1649512-setnotificationcategories)
    /// - [UNUserNotificationCenterDelegate](https://developer.apple.com/documentation/usernotifications/unusernotificationcenterdelegate)
    fn register(
        &self,
        handler_callback: Box<dyn Fn(crate::NotifyResponse) + Send + Sync + 'static>,
        categories: Vec<NotifyCategory>,
    ) -> Result<(), crate::Error> {
        let mtm = MainThreadMarker::new().ok_or(Error::NotMainThread)?;
        let (tx, mut rx) =
            tokio::sync::mpsc::channel::<NotifyResponse>(NOTIFICATION_RESPONSE_CHANNEL_SIZE);
        let notification_delegate = NotificationDelegate::new(mtm, tx);

        unsafe {
            // Create and set the delegate
            let proto: Retained<ProtocolObject<dyn UNUserNotificationCenterDelegate>> =
                ProtocolObject::from_retained(notification_delegate);

            let notification_center = UNUserNotificationCenter::currentNotificationCenter();
            notification_center.setDelegate(Some(&*proto));

            // Store delegate reference to prevent deallocation
            self.inner
                .delegate_reference
                .set(proto)
                .map_err(|_| Error::MultipleRegisterCalls)?;

            // Register notification categories
            let categories: Retained<NSSet<_>> = categories
                .into_iter()
                .map(|category| W(category_to_native_category(category)))
                .collect();
            notification_center.setNotificationCategories(&categories);

            // Start the response handler thread
            let handler_loop = thread::spawn(move || {
                while let Some(response) = rx.blocking_recv() {
                    handler_callback(response)
                }
            });

            self.inner
                .listener_loop
                .set(handler_loop)
                .map_err(|_| Error::MultipleRegisterCalls)?;
        }

        Ok(())
    }

    /// Removes all delivered notifications from the notification center
    ///
    /// # Errors
    /// - `Error::NoBundleId` if the app doesn't have a valid bundle identifier
    ///
    /// # References
    /// - [removeAllDeliveredNotifications](https://developer.apple.com/documentation/usernotifications/unusernotificationcenter/1649501-removealldeliverednotifications)
    fn remove_all_delivered_notifications(&self) -> Result<(), Error> {
        self.ensure_valid_bundle_id()?;

        unsafe {
            UNUserNotificationCenter::currentNotificationCenter().removeAllDeliveredNotifications();
        }

        Ok(())
    }

    /// Removes specific delivered notifications by their identifiers
    ///
    /// # Arguments
    /// * `ids` - Vector of notification identifiers to remove
    ///
    /// # Errors
    /// - `Error::NoBundleId` if the app doesn't have a valid bundle identifier
    ///
    /// # References
    /// - [removeDeliveredNotificationsWithIdentifiers](https://developer.apple.com/documentation/usernotifications/unusernotificationcenter/1649500-removedeliverednotificationswith)
    fn remove_delivered_notifications(&self, ids: Vec<&str>) -> Result<(), Error> {
        self.remove_notifications_by_ids(ids)
    }

    /// Retrieves all currently active (delivered) notifications
    ///
    /// # Returns
    /// Vector of notification handles for all active notifications
    ///
    /// # Errors
    /// - `Error::NoBundleId` if the app doesn't have a valid bundle identifier
    /// - Communication errors from async channels
    ///
    /// # References
    /// - [getDeliveredNotifications](https://developer.apple.com/documentation/usernotifications/unusernotificationcenter/1649520-getdeliverednotifications)
    async fn get_active_notifications(&self) -> Result<Vec<Self::NotifyHandle>, Error> {
        self.ensure_valid_bundle_id()?;

        let (tx, rx) = tokio::sync::oneshot::channel::<Vec<NotifyHandle>>();

        {
            let completion_handler = Self::create_notifications_handler(tx);
            unsafe {
                UNUserNotificationCenter::currentNotificationCenter()
                    .getDeliveredNotificationsWithCompletionHandler(&completion_handler);
            }
        }

        Ok(rx.await?)
    }

    /// Sends a notification using the provided builder configuration
    ///
    /// # Arguments
    /// * `builder` - Configuration for the notification to send
    ///
    /// # Returns
    /// A handle to the sent notification
    ///
    /// # Errors
    /// - Various errors from the notification building and sending process
    /// - Communication errors from async channels
    ///
    /// # References
    /// - [UNUserNotificationCenter.addNotificationRequest](https://developer.apple.com/documentation/usernotifications/unusernotificationcenter/1649508-addnotificationrequest)
    async fn send(&self, builder: NotifyBuilder) -> Result<Self::NotifyHandle, Error> {
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<(), Error>>();
        let handle = build_and_send(builder, self, tx)?;
        rx.await??;
        Ok(handle)
    }
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Converts a UserNotifications userInfo dictionary to a Rust HashMap
///
/// This function safely extracts string key-value pairs from the native
/// NSDictionary format used by the UserNotifications framework.
///
/// # Arguments
/// * `user_info` - The native userInfo dictionary from a notification
///
/// # Returns
/// A HashMap containing string key-value pairs
///
/// # References
/// - [UNNotificationContent.userInfo](https://developer.apple.com/documentation/usernotifications/unnotificationcontent/1649866-userinfo)
pub(crate) fn user_info_dictionary_to_hashmap(
    user_info: Retained<NSDictionary<AnyObject, AnyObject>>,
) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let keys = user_info.allKeys();

    for key in keys {
        if let Some(key_ns_string) = key.downcast_ref::<NSString>() {
            if let Some(value) = user_info.objectForKey(key.deref()) {
                if let Some(value_ns_string) = value.downcast_ref::<NSString>() {
                    map.insert(key_ns_string.to_string(), value_ns_string.to_string());
                } else {
                    log::error!("value object failed to downcast to ns_string: {value:?}");
                }
            } else {
                log::error!("no value found for key {key:?}");
            }
        } else {
            log::error!("key object failed to downcast to ns_string: {key:?}");
        }
    }

    map
}

/// Converts a cross-platform NotifyCategory to a native UNNotificationCategory
///
/// This function transforms our platform-agnostic category representation
/// into the native macOS format, including action conversion.
///
/// # Arguments
/// * `category` - The cross-platform category definition
///
/// # Returns
/// A native UNNotificationCategory ready for registration
///
/// # References
/// - [UNNotificationCategory](https://developer.apple.com/documentation/usernotifications/unnotificationcategory)
/// - [UNNotificationAction](https://developer.apple.com/documentation/usernotifications/unnotificationaction)
fn category_to_native_category(category: NotifyCategory) -> Retained<UNNotificationCategory> {
    let identifier = NSString::from_str(&category.identifier);

    let actions: Retained<_> = category
        .actions
        .iter()
        .map(convert_action_to_native)
        .collect();

    unsafe {
        UNNotificationCategory::categoryWithIdentifier_actions_intentIdentifiers_options(
            &identifier,
            &actions,
            &NSArray::new(),
            UNNotificationCategoryOptions::empty(),
        )
    }
}

/// Converts a single notification action to its native representation
///
/// # Arguments
/// * `action` - The cross-platform action definition
///
/// # Returns
/// A wrapped native notification action
///
/// # References
/// - [UNNotificationAction](https://developer.apple.com/documentation/usernotifications/unnotificationaction)
/// - [UNTextInputNotificationAction](https://developer.apple.com/documentation/usernotifications/untextinputnotificationaction)
fn convert_action_to_native(action: &crate::NotifyCategoryAction) -> W<UNNotificationAction> {
    use crate::NotifyCategoryAction::*;

    match action {
        Action { identifier, title } => {
            let identifier = NSString::from_str(identifier);
            let title = NSString::from_str(title);
            unsafe {
                W(UNNotificationAction::actionWithIdentifier_title_options(
                    &identifier,
                    &title,
                    UNNotificationActionOptions::empty(),
                ))
            }
        }
        TextInputAction {
            identifier,
            title,
            input_button_title,
            input_placeholder,
        } => {
            let identifier = NSString::from_str(identifier);
            let title = NSString::from_str(title);
            let text_input_button_title = NSString::from_str(input_button_title);
            let text_input_placeholder = NSString::from_str(input_placeholder);
            unsafe {
                W(Retained::cast_unchecked::<UNNotificationAction>(
                    UNTextInputNotificationAction::actionWithIdentifier_title_options_textInputButtonTitle_textInputPlaceholder(
                        &identifier,
                        &title,
                        UNNotificationActionOptions::empty(),
                        &text_input_button_title,
                        &text_input_placeholder
                    )
                ))
            }
        }
    }
}

// ============================================================================
// Helper Types and Implementations
// ============================================================================

/// Wrapper type to bypass Rust's orphan rule for implementing traits
///
/// This allows us to implement `FromIterator` for `Retained<NSArray<O>>`
/// and `Retained<NSSet<O>>` in this crate.
struct W<T: ?Sized + Message>(Retained<T>);

/// Implements collection conversion for NSArray
///
/// This allows using the standard iterator collection methods to create
/// native NSArray instances from our wrapped types.
impl<O: Message> FromIterator<W<O>> for Retained<NSArray<O>> {
    fn from_iter<T: IntoIterator<Item = W<O>>>(iter: T) -> Self {
        let vec: Vec<Retained<O>> = iter.into_iter().map(|o| o.0).collect();

        let array: Retained<NSArray<O>> = NSArray::from_slice(
            vec.iter()
                .map(|r| r.deref())
                .collect::<Vec<&O>>()
                .as_slice(),
        );
        array
    }
}

/// Implements collection conversion for NSSet
///
/// This allows using the standard iterator collection methods to create
/// native NSSet instances from our wrapped types.
impl<O: Message> FromIterator<W<O>> for Retained<NSSet<O>> {
    fn from_iter<T: IntoIterator<Item = W<O>>>(iter: T) -> Self {
        let vec: Vec<Retained<O>> = iter.into_iter().map(|o| o.0).collect();

        let set: Retained<NSSet<O>> = NSSet::from_slice(
            vec.iter()
                .map(|r| r.deref())
                .collect::<Vec<&O>>()
                .as_slice(),
        );
        set
    }
}
