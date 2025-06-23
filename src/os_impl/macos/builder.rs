use std::{collections::HashMap, ops::Deref};

use super::{NotifyHandleMacOS, NotifyManagerMacOS};
use objc2::{rc::Retained, runtime::AnyObject};
use objc2_foundation::{NSDictionary, NSString};
use objc2_user_notifications::{
    UNMutableNotificationContent, UNNotificationRequest, UNNotificationSound,
};
use uuid::Uuid;

use crate::{Error, NotifyBuilder};

pub(super) fn build_and_send(
    builder: NotifyBuilder,
    manager: &NotifyManagerMacOS,
    tx: tokio::sync::oneshot::Sender<Result<(), Error>>,
) -> Result<NotifyHandleMacOS, Error> {
    let (request, id, user_info) = build(builder, manager)?;
    manager.add_notification(&request, move |result| {
        if let Err(err) = tx.send(result) {
            log::error!("add_notification tx.send error {err:?}");
        }
    });
    Ok(NotifyHandleMacOS::new(id, user_info))
}

#[allow(clippy::type_complexity)]
fn build(
    builder: NotifyBuilder,
    manager: &NotifyManagerMacOS,
) -> Result<
    (
        Retained<UNNotificationRequest>,
        String,
        HashMap<String, String>,
    ),
    Error,
> {
    let mut user_info = HashMap::new();

    let notification: Retained<UNMutableNotificationContent> = unsafe {
        let notification = UNMutableNotificationContent::new();

        if let Some(body) = builder.body {
            notification.setBody(&NSString::from_str(&body));
        }

        if let Some(title) = builder.title {
            notification.setTitle(&NSString::from_str(&title));
        }

        if let Some(subtitle) = builder.subtitle {
            notification.setSubtitle(&NSString::from_str(&subtitle));
        }

        if let Some(sound_name) = builder.sound {
            let sound = if sound_name == "default" {
                UNNotificationSound::defaultSound()
            } else {
                UNNotificationSound::soundNamed(&NSString::from_str(&sound_name))
            };
            notification.setSound(Some(&sound));
        } else {
            notification.setSound(Some(&UNNotificationSound::defaultSound()));
        }

        if let Some(thread_id) = builder.thread_id {
            notification.setThreadIdentifier(&NSString::from_str(&thread_id));
        }
        if let Some(category_id) = builder.category_id {
            notification.setCategoryIdentifier(&NSString::from_str(&category_id));
        }

        if let Some(payload) = builder.user_metadata {
            let mut user_info_keys = Vec::with_capacity(payload.len());
            let mut user_info_values = Vec::with_capacity(payload.len());
            for (key, value) in payload.iter() {
                user_info_keys.push(NSString::from_str(key));
                user_info_values.push(NSString::from_str(value));
            }
            let string_dictionary = NSDictionary::from_slices(
                user_info_keys
                    .iter()
                    .map(|r| r.deref())
                    .collect::<Vec<&NSString>>()
                    .as_slice(),
                user_info_values
                    .iter()
                    .map(|r| r.deref())
                    .collect::<Vec<&NSString>>()
                    .as_slice(),
            );
            let anyobject_dictionary =
                Retained::cast_unchecked::<NSDictionary<AnyObject, AnyObject>>(string_dictionary);
            notification.setUserInfo(anyobject_dictionary.deref());
            user_info = payload;
        }

        notification
    };

    unsafe {
        let bundle_id = manager
            .inner
            .bundle_id
            .as_ref()
            .map(|s| NSString::from_str(s))
            .ok_or(Error::NoBundleId)?;

        let id = format!("{}.{}", Uuid::new_v4(), bundle_id);

        let r = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &NSString::from_str(&id),
            &notification,
            None,
        );

        Ok((r, id, user_info))
    }
}
