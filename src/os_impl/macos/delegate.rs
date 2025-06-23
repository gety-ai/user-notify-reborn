use crate::{NotifyResponse, NotifyResponseAction, macos::user_info_dictionary_to_hashmap};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained};
use objc2_foundation::{NSObject, NSObjectProtocol};
use objc2_user_notifications::{
    UNNotification, UNNotificationDefaultActionIdentifier, UNNotificationDismissActionIdentifier,
    UNNotificationPresentationOptions, UNNotificationResponse, UNTextInputNotificationResponse,
    UNUserNotificationCenter, UNUserNotificationCenterDelegate,
};
use tokio::sync::mpsc::Sender;

#[derive(Clone)]
pub struct Ivars {
    pub sender: Sender<NotifyResponse>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[ivars = Ivars]
    #[name = "UserNotifyRebornDelegate"]
    #[thread_kind = MainThreadOnly]
    pub struct NotificationDelegate;

    impl NotificationDelegate {}

    unsafe impl NSObjectProtocol for NotificationDelegate {}

    unsafe impl UNUserNotificationCenterDelegate for NotificationDelegate {
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present_notification(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            completion_handler: &block2::Block<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            log::debug!("macOS: Will present notification");
            let presentation_options = UNNotificationPresentationOptions::empty()
                .union(UNNotificationPresentationOptions::Badge)
                .union(UNNotificationPresentationOptions::Banner)
                .union(UNNotificationPresentationOptions::Sound);
            completion_handler.call((presentation_options,));
        }

        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        unsafe fn did_receive_notification_response(
            &self,
            _center: &UNUserNotificationCenter,
            response: &UNNotificationResponse,
            completion_handler: &block2::Block<dyn Fn()>,
        ) {
            log::debug!("macOS: Did receive notification response");

            unsafe {
                let action_id = response.actionIdentifier();
                let action: NotifyResponseAction = match &*action_id {
                    a if a == UNNotificationDefaultActionIdentifier => NotifyResponseAction::Default,
                    a if a == UNNotificationDismissActionIdentifier => NotifyResponseAction::Dismiss,
                    _ => NotifyResponseAction::Other(action_id.to_string()),
                };

                let user_input = response
                    .downcast_ref::<UNTextInputNotificationResponse>()
                    .map(|text_response| text_response.userText().to_string());

                let notification = response.notification();
                let request = notification.request();
                let notification_id = request.identifier().to_string();
                let user_metadata = user_info_dictionary_to_hashmap(request.content().userInfo());

                let event = NotifyResponse {
                    notification_id,
                    action,
                    user_input,
                    user_metadata,
                };

                if let Err(err) = self.ivars().sender.try_send(event) {
                    log::error!("Failed to send notification to handler: {err:?}");
                }
            }

            completion_handler.call(());
        }
    }
);

impl NotificationDelegate {
    pub fn new(mtm: MainThreadMarker, tx: Sender<NotifyResponse>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars { sender: tx });
        unsafe { msg_send![super(this), init] }
    }
}
