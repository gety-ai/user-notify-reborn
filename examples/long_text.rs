use tokio::time::{Duration, sleep};
use user_notify_reborn::{NotifyBuilder, NotifyManagerFactory};

const DEFAULT_BUNDLE_ID: &str = "com.example.user-notify-reborn";

fn get_test_bundle_id() -> String {
    std::env::var("TEST_BUNDLE_ID").unwrap_or_else(|_| DEFAULT_BUNDLE_ID.to_string())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let bundle_id = get_test_bundle_id();

    let manager = NotifyManagerFactory::new(bundle_id, Some("usernotify".to_string()))?;

    // Register notification handler
    manager.register(
        Box::new(|response| {
            println!("📳 Received notification response: {response:?}");
        }),
        vec![],
    )?;

    // Request permission (important on macOS)
    let permission = manager.first_time_ask_for_notification_permission().await?;
    println!("🔐 Notification permission granted: {}", permission);

    // Send first notification
    println!("📤 Sending first notification...");
    let notification1 = NotifyBuilder::new()
        .title("Active Test - First Notification")
        .body("This is the first notification for active testing")
        .subtitle("First Test")
        .sound("default");

    let handle1 = manager.send(notification1).await?;
    println!("✅ First notification sent with ID: {}", handle1.get_id());

    // Wait a bit
    sleep(Duration::from_secs(2)).await;

    // Send second notification
    println!("📤 Sending second notification...");
    let notification2 = NotifyBuilder::new()
        .title("Active Test - Second Notification")
        .body("This is the second notification for active testing")
        .subtitle("Second Test")
        .sound("default");

    let handle2 = manager.send(notification2).await?;
    println!("✅ Second notification sent with ID: {}", handle2.get_id());

    // Wait for notifications to be processed
    println!("⏱️ Waiting for notifications to be processed...");
    sleep(Duration::from_secs(3)).await;

    // Get active notifications
    println!("📋 Getting list of active notifications...");
    let active = manager.get_active_notifications().await?;
    println!("📊 Found {} active notifications", active.len());

    for (i, handle) in active.iter().enumerate() {
        println!("🔍 Notification {}: ID = {}", i + 1, handle.get_id());
    }

    println!("✅ Found {} active notifications", active.len());

    if active.is_empty() {
        println!("⚠️ No active notifications found. They may have been dismissed or expired.");
    } else {
        println!("🎯 Successfully verified active notification management!");
    }

    println!("💡 You can check your system notification center to see the active notifications");
    println!("🎉 Active notifications test completed!");

    // Clean up
    manager.remove_all_delivered_notifications()?;

    Ok(())
}
