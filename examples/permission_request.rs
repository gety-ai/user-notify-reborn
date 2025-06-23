use user_notify_reborn::{NotifyCategory, NotifyCategoryAction, NotifyManagerFactory};

const DEFAULT_BUNDLE_ID: &str = "com.example.user-notify-reborn";
const ACTION_CATEGORY_ID: &str = "app.category.action";

fn get_test_bundle_id() -> String {
    std::env::var("TEST_BUNDLE_ID").unwrap_or_else(|_| DEFAULT_BUNDLE_ID.to_string())
}

fn create_test_categories() -> Vec<NotifyCategory> {
    vec![NotifyCategory {
        identifier: ACTION_CATEGORY_ID.to_string(),
        actions: vec![
            NotifyCategoryAction::Action {
                identifier: format!("{}.button.submit", ACTION_CATEGORY_ID),
                title: "Submit".to_string(),
            },
            NotifyCategoryAction::Action {
                identifier: format!("{}.button.cancel", ACTION_CATEGORY_ID),
                title: "Cancel".to_string(),
            },
        ],
    }]
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let bundle_id = get_test_bundle_id();

    let manager = NotifyManagerFactory::new(bundle_id, None)?;
    let categories = create_test_categories();

    // Register categories first
    manager.register(
        Box::new(|response| {
            println!("📳 Received notification response: {response:?}");
        }),
        categories,
    )?;

    // Request permission
    #[cfg(target_os = "macos")]
    {
        println!("🔐 Requesting notification permission...");
        match manager.first_time_ask_for_notification_permission().await {
            Ok(_) => println!("✅ Permission request completed successfully"),
            Err(err) => {
                println!("❌ Permission request failed: {err:?}");
                return Err(err.into());
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        println!("💡 Non-macOS platform, permission request not required");
    }

    println!("🎉 Permission request test completed!");
    Ok(())
}
