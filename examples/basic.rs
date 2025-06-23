use user_notify_reborn::{NotifyBuilder, NotifyManagerFactory};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    // Create notification manager
    let manager = NotifyManagerFactory::new(
        "com.example.user-notify-reborn".to_string(),
        Some("usernotify".to_string()),
    )?;

    // Register notification handler
    manager.register(
        Box::new(|response| {
            println!("Received notification response: {:?}", response);
        }),
        vec![],
    )?;

    // Request permission (important on macOS)
    let permission = manager.first_time_ask_for_notification_permission().await?;
    println!("Notification permission granted: {}", permission);

    let notification = NotifyBuilder::new()
        .title("Hello from user-notify-reborn!")
        .body("This is a test notification from the reborn library.")
        .subtitle("Test Subtitle")
        .sound("default");

    // Send the notification
    let handle = manager.send(notification).await?;
    println!("Notification sent with ID: {}", handle.get_id());

    // Wait a bit to see the notification
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    // Get active notifications
    let active = manager.get_active_notifications().await?;
    println!("Active notifications: {}", active.len());

    // Clean up
    manager.remove_all_delivered_notifications()?;

    Ok(())
}
