use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    // macOS errors
    #[cfg(target_os = "macos")]
    #[error("bundle id is not set, this is required to send notifications")]
    NoBundleId,
    #[cfg(target_os = "macos")]
    #[error("macOS APIs need to be called from the main thread, but this is not the main thread")]
    NotMainThread,
    #[cfg(target_os = "macos")]
    #[error("NSError: code: {code}, domain: {domain}, user_info: {user_info:?}, description: {description}")]
    NSError {
        code: isize,
        domain: String,
        user_info: String,
        description: String,
    },
    #[cfg(target_os = "macos")]
    #[error("Failed to set delegate_reference, did you call register multiple times?")]
    MultipleRegisterCalls,
    #[cfg(target_os = "macos")]
    #[error("Failed to set listener_loop, did you call register multiple times?")]
    MultipleRegisterCallsListenerLoop,

    // Windows errors
    #[cfg(target_os = "windows")]
    #[error(transparent)]
    Windows(#[from] windows::core::Error),
    #[cfg(target_os = "windows")]
    #[error("Failed to parse user info {0:?}")]
    FailedToParseUserInfo(serde_json::Error),
    #[cfg(target_os = "windows")]
    #[error("Error Setting Handler Callback")]
    SettingHandler,
    #[cfg(target_os = "windows")]
    #[error(transparent)]
    XmlEscape(#[from] quick_xml::escape::EscapeError),
    #[cfg(target_os = "windows")]
    #[error(transparent)]
    UrlParse(#[from] url::ParseError),
    #[cfg(target_os = "windows")]
    #[error(transparent)]
    Base64Decode(#[from] base64::DecodeError),

    // Common errors
    #[error("Infallible error, something went really wrong: {0}")]
    Infallible(#[from] std::convert::Infallible),
    #[error(transparent)]
    TokioRecv(#[from] tokio::sync::oneshot::error::RecvError),
    #[error(transparent)]
    TokioTryLock(#[from] tokio::sync::TryLockError),
    #[error("Url from path parse error {0:?}")]
    ParseUrlFromPath(PathBuf),
    #[error("Other error: {0}")]
    Other(String),
}


#[cfg(target_os = "macos")]
impl From<&objc2_foundation::NSError> for Error {
    fn from(error: &objc2_foundation::NSError) -> Self {
        Error::NSError {
            code: error.code(),
            domain: error.domain().to_string(),
            user_info: format!("{:?}", error.userInfo()),
            description: error.localizedDescription().to_string(),
        }
    }
}
