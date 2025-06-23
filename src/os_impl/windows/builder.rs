use crate::{Error, NotifyBuilder, NotifyResponse, NotifyResponseAction};
use base64::Engine;
use std::collections::HashMap;
use windows::{Data::Xml::Dom::XmlDocument, core::HSTRING};

/// Builds Windows Toast notification XML from a NotifyBuilder.
///
/// This function constructs the XML document that Windows uses to display toast notifications.
/// The XML follows the Toast content schema defined by Microsoft.
///
/// # References
/// - [Toast content schema](https://docs.microsoft.com/en-us/windows/apps/design/shell/tiles-and-notifications/toast-schema)
/// - [ToastGeneric template](https://docs.microsoft.com/en-us/windows/apps/design/shell/tiles-and-notifications/adaptive-interactive-toasts)
pub fn build_toast_xml(
    builder: NotifyBuilder,
    id: &str,
    notification_protocol: Option<&str>,
    generate_actions_xml_fn: impl Fn(&str) -> Result<String, Error>,
) -> Result<XmlDocument, Error> {
    let title_content = builder
        .title
        .map(|title| {
            format!(
                r#"<text id="1">{}</text>"#,
                quick_xml::escape::escape(title)
            )
        })
        .unwrap_or_default();

    let subtitle_content = builder
        .subtitle
        .map(|subtitle| {
            format!(
                r#"<text id="2">{}</text>"#,
                quick_xml::escape::escape(subtitle)
            )
        })
        .unwrap_or_default();

    let body_content = builder
        .body
        .map(|body| format!(r#"<text id="3">{}</text>"#, quick_xml::escape::escape(body)))
        .unwrap_or_default();

    let launch_options = if let Some(notification_protocol) = notification_protocol {
        let launch_url = encode_deeplink(
            notification_protocol,
            &NotifyResponse {
                notification_id: id.to_string(),
                action: NotifyResponseAction::Default,
                user_input: None,
                user_metadata: builder.user_metadata.clone().unwrap_or_default(),
            },
        );
        format!(r#"launch="{launch_url}" activationType="protocol""#)
    } else {
        String::new()
    };

    // Generate actions XML based on category
    let actions_xml = if let Some(category_id) = &builder.category_id {
        generate_actions_xml_fn(category_id)?
    } else {
        String::new()
    };

    // TODO: support custom sound
    // - [Toast audio options](https://docs.microsoft.com/en-us/windows/apps/design/shell/tiles-and-notifications/custom-audio-on-toasts)
    let toast_xml = XmlDocument::new()?;
    toast_xml
        .LoadXml(&HSTRING::from(format!(
            r#"<toast duration="short" {launch_options}>
            <visual>
                <binding template="ToastGeneric">
                    {title_content}
                    {subtitle_content}
                    {body_content}
                </binding>
            </visual>
            <audio src="ms-winsoundevent:Notification.SMS" />
            {actions_xml}
        </toast>"#
        )))
        .map_err(|e| Error::Other(e.to_string()))?;

    Ok(toast_xml)
}

/// Encodes a custom protocol deeplink for notification activation.
///
/// This creates a URL that can be used to handle notification responses through a custom protocol.
/// The user metadata is Base64-encoded to safely include complex data in the URL.
///
/// # References
/// - [Launch your app with a URI](https://docs.microsoft.com/en-us/windows/uwp/launch-resume/launch-app-with-uri)
/// - [Handle app activation](https://docs.microsoft.com/en-us/windows/apps/design/shell/tiles-and-notifications/send-local-toast-desktop)
/// - [Base64 encoding specification (RFC 4648)](https://tools.ietf.org/html/rfc4648)
fn encode_deeplink(scheme: &str, action: &NotifyResponse) -> String {
    let user_metadata_string = match serde_json::to_string(&action.user_metadata) {
        Ok(user_metadata_string) => Some(user_metadata_string),
        Err(err) => {
            log::error!(
                "failed to serialize user_metadata: ({:?}) {:?}",
                action.user_metadata,
                err
            );
            None
        }
    }
    .unwrap_or_else(|| "{}".to_string());

    let attribute = base64::prelude::BASE64_STANDARD.encode(&user_metadata_string);

    let action_string = match &action.action {
        NotifyResponseAction::Default => "__default__",
        NotifyResponseAction::Dismiss => "__dismiss__",
        NotifyResponseAction::Other(action) => action.as_ref(),
    };

    format!(
        "{scheme}://{}/{}?{attribute}",
        action.notification_id, action_string
    )
}

/// Decodes a custom protocol deeplink back into a NotifyResponse.
///
/// This function parses URLs created by `encode_deeplink()` to extract notification
/// response information when the application is activated through the protocol handler.
///
/// # References
/// - [url crate documentation](https://docs.rs/url/latest/url/)
/// - [URL parsing specification (RFC 3986)](https://tools.ietf.org/html/rfc3986)
/// - [Base64 decoding specification (RFC 4648)](https://tools.ietf.org/html/rfc4648)
pub fn decode_deeplink(link: &str) -> Result<NotifyResponse, Error> {
    let url = url::Url::parse(link)?;

    let user_metadata: HashMap<String, String> = match url.query() {
        None => {
            log::error!("notification deeplink has no user info");
            HashMap::new()
        }
        Some(base64_userinfo) => {
            let user_info_str = base64::prelude::BASE64_STANDARD.decode(base64_userinfo)?;
            serde_json::from_slice(user_info_str.as_slice())
                .map_err(Error::FailedToParseUserInfo)?
        }
    };

    Ok(NotifyResponse {
        notification_id: url.host().map(|host| host.to_string()).unwrap_or_default(),
        action: match url.path().to_string().as_str() {
            "/__default__" => NotifyResponseAction::Default,
            "/__dismiss__" => NotifyResponseAction::Dismiss,
            action => NotifyResponseAction::Other(action.to_owned()),
        },
        user_input: None,
        user_metadata,
    })
}
