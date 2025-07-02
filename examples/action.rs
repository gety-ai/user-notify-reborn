use std::collections::HashMap;
use tokio::time::{sleep, Duration};
use user_notify_reborn::prelude::*;

const DEFAULT_BUNDLE_ID: &str = "ai.gety.test.full";
const ACTION_CATEGORY_ID: &str = "app.category.action";
const TEXT_INPUT_CATEGORY_ID: &str = "app.category.textinput";

fn init_logger() {
    let _ = env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .is_test(false)
        .init();
}

fn get_test_bundle_id() -> String {
    std::env::var("TEST_BUNDLE_ID").unwrap_or_else(|_| DEFAULT_BUNDLE_ID.to_string())
}

fn create_test_categories() -> Vec<NotifyCategory> {
    vec![
        NotifyCategory {
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
                NotifyCategoryAction::Action {
                    identifier: format!("{}.button.detail", ACTION_CATEGORY_ID),
                    title: "Detail".to_string(),
                },
            ],
        },
        NotifyCategory {
            identifier: TEXT_INPUT_CATEGORY_ID.to_string(),
            actions: vec![NotifyCategoryAction::TextInputAction {
                identifier: format!("{}.button.send", TEXT_INPUT_CATEGORY_ID),
                title: "Reply".to_string(),
                input_button_title: "Send".to_string(),
                input_placeholder: "Type your message here...".to_string(),
            }],
        },
    ]
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_logger();
    println!("🚀 Starting full integration test...");
    println!("🎯 This test will demonstrate all notification features:");
    println!("   • Permission request");
    println!("   • Category registration");
    println!("   • Basic notifications");
    println!("   • Interactive notifications");
    println!("   • Active notification management");
    println!();

    let bundle_id = get_test_bundle_id();
    println!("📱 Using Bundle ID: {}", bundle_id);

    let manager = NotifyManager::try_new(&bundle_id, Some("usernotify"))?;
    let categories = create_test_categories();

    // Step 1: Register categories FIRST
    println!("📝 Step 1: Registering notification categories...");
    manager.register(
        Box::new(|response| {
            println!("📳 Received notification response: {response:?}");
        }),
        categories,
    )?;
    println!("✅ Categories registered successfully");

    // Wait a bit for categories to be processed by the system
    sleep(Duration::from_millis(500)).await;
    println!();

    // Step 2: Request permission and WAIT for result
    println!("🔐 Step 2: Requesting notification permission...");
    let permission_granted = match manager.first_time_ask_for_notification_permission().await {
        Ok(granted) => {
            if granted {
                println!("✅ Permission granted successfully!");
                true
            } else {
                println!("❌ Permission was denied by user");
                false
            }
        }
        Err(err) => {
            println!("⚠️ Permission request failed: {err:?}");
            #[cfg(target_os = "macos")]
            {
                println!("💡 On macOS, you may need to:");
                println!("   1. Open System Preferences/Settings");
                println!("   2. Go to Notifications & Focus");
                println!("   3. Find this app and enable notifications");
                return Err(err.into());
            }
            #[cfg(not(target_os = "macos"))]
            {
                println!("💡 Continuing anyway on non-macOS platform");
                true
            }
        }
    };

    if !permission_granted {
        println!("❌ Cannot proceed without notification permission");
        return Ok(());
    }

    // Wait for permission to fully take effect
    println!("⏳ Waiting for permission to take effect...");
    sleep(Duration::from_secs(2)).await;
    println!();

    // Step 3: Send basic notification with actions
    println!("📤 Step 3: Sending basic notification with action buttons...");
    println!("💡 Look for Submit, Cancel, and Detail buttons on the notification!");
    let notification1 = NotifyBuilder::new()
        .title("Integration Test - Actions")
        .body("This notification has action buttons - try clicking them!")
        .set_thread_id("integration-thread-basic")
        .set_category_id(ACTION_CATEGORY_ID);

    let handle1 = manager.send(notification1).await?;
    println!("✅ Basic notification sent with ID: {}", handle1.get_id());
    sleep(Duration::from_secs(3)).await;
    println!();

    // Step 4: Send notification with text input
    println!("📤 Step 4: Sending notification with text input...");
    println!("💡 Look for a Reply button that allows text input!");
    let mut user_info = HashMap::new();
    user_info.insert("integration_test".to_owned(), "full_flow".to_owned());
    user_info.insert("step".to_owned(), "4".to_owned());
    user_info.insert(
        "timestamp".to_owned(),
        format!(
            "{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        ),
    );

    let notification2 = NotifyBuilder::new()
        .title("Integration Test - Text Input")
        .body("This notification has text input - try replying to it!")
        .set_thread_id("integration-thread-interactive")
        .set_user_metadata(user_info.clone())
        .set_category_id(TEXT_INPUT_CATEGORY_ID);

    let handle2 = manager.send(notification2).await?;
    println!(
        "✅ Interactive notification sent with ID: {}",
        handle2.get_id()
    );
    sleep(Duration::from_secs(3)).await;
    println!();

    // Step 5: Check active notifications
    println!("📋 Step 5: Checking active notifications...");
    let active = manager.get_active_notifications().await?;
    println!("📊 Found {} active notifications", active.len());

    for (i, handle) in active.iter().enumerate() {
        println!("🔍 Notification {}: ID = {}", i + 1, handle.get_id());
    }

    if !active.is_empty() {
        println!(
            "✅ Successfully found {} notifications in active list",
            active.len()
        );
    } else {
        println!("⚠️ No notifications found in active list - they may have been dismissed");
    }
    println!();

    // Step 6: Send a final notification with actions
    println!("📤 Step 6: Sending completion notification with actions...");
    let notification3 = NotifyBuilder::new()
        .title("Integration Test Complete! 🎉")
        .body("All features tested! Try the action buttons to see responses in console.")
        .set_thread_id("integration-thread-complete")
        .set_category_id(ACTION_CATEGORY_ID);

    let handle3 = manager.send(notification3).await?;
    println!(
        "✅ Completion notification sent with ID: {}",
        handle3.get_id()
    );
    println!();

    println!("🎊 Full integration test completed successfully!");
    println!();
    println!("📋 IMPORTANT NOTES:");
    println!("🔔 Check your system notification center to see all the notifications");
    println!("🎯 Try clicking the action buttons on notifications:");
    println!("   • Submit, Cancel, Detail buttons on regular notifications");
    println!("   • Reply button with text input on interactive notifications");
    println!("📳 Watch the console for response messages when you interact with notifications");

    #[cfg(target_os = "macos")]
    {
        println!();
        println!("🍎 macOS Tips:");
        println!("   • Notifications appear in Notification Center (top-right corner)");
        println!("   • You can also see them by swiping left from the right edge");
        println!("   • If buttons don't appear, check System Settings > Notifications");
    }

    #[cfg(target_os = "windows")]
    {
        println!();
        println!("🪟 Windows Tips:");
        println!("   • Notifications appear in Action Center (bottom-right corner)");
        println!("   • Press Win+A to open Action Center manually");
        println!("   • Buttons should appear below the notification text");
    }

    // Keep the program running for a longer time to handle responses
    println!();
    println!("⏱️ Keeping program alive for 60 seconds to handle notification responses...");
    println!("💡 Try interacting with the notifications now!");

    for remaining in (1..=60).rev() {
        if remaining % 10 == 0 || remaining <= 5 {
            println!("⏳ {} seconds remaining...", remaining);
        }
        sleep(Duration::from_secs(1)).await;
    }

    println!("👋 Test program finishing. Thank you!");
    Ok(())
}
