use std::collections::HashMap;
use tokio::time::{sleep, Duration};
use user_notify_reborn::prelude::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Debug)
        .init();

    println!("🔧 Windows COM Requirements Test");
    println!("==================================");
    println!();
    println!("This test helps diagnose why action buttons may not appear in Windows Toast notifications.");
    println!();

    // 1. Test without categories (should work)
    println!("📋 Test 1: Basic notification without actions");
    let manager = NotifyManager::try_new("com.test.basic", None)?;
    
    let basic_notification = NotifyBuilder::new()
        .title("Basic Test")
        .body("This notification has no action buttons and should display normally.");
    
    let handle = manager.send(basic_notification).await?;
    println!("✅ Basic notification sent with ID: {}", handle.get_id());
    
    sleep(Duration::from_secs(3)).await;

    // 2. Test with categories (may not show buttons due to COM requirements)
    println!();
    println!("📋 Test 2: Notification with action buttons");
    
    let categories = vec![NotifyCategory {
        identifier: "test.category".to_string(),
        actions: vec![NotifyCategoryAction::Action {
            identifier: "test.action".to_string(),
            title: "Click Me".to_string(),
        }],
    }];

    manager.register(
        Box::new(|response| {
            println!("🎯 Action clicked: {:#?}", response);
        }),
        categories,
    )?;

    let action_notification = NotifyBuilder::new()
        .title("Action Test")
        .body("This notification should have a 'Click Me' button.")
        .set_category_id("test.category");
    
    let handle = manager.send(action_notification).await?;
    println!("✅ Action notification sent with ID: {}", handle.get_id());
    
    println!();
    println!("🔍 DIAGNOSIS:");
    println!("1. If you see the first notification but no button on the second notification,");
    println!("   this confirms the Windows COM Activator requirement issue.");
    println!();
    println!("2. For Win32 applications (like Rust), Windows requires:");
    println!("   - COM Server registration in registry");
    println!("   - Application shortcut with specific properties");
    println!("   - Proper AUMID (Application User Model ID)");
    println!();
    println!("3. Without these components, Windows will:");
    println!("   - Show basic notifications");
    println!("   - Strip interactive elements (buttons/inputs)");
    println!("   - Not persist notifications in Action Center properly");
    println!();
    println!("Waiting 15 seconds to observe both notifications...");
    
    for i in (1..=15).rev() {
        print!("\r⏳ {} seconds remaining", i);
        std::io::Write::flush(&mut std::io::stdout()).unwrap();
        sleep(Duration::from_secs(1)).await;
    }
    
    println!();
    println!();
    println!("📝 SOLUTION:");
    println!("To enable action buttons in Windows Toast notifications for Rust applications:");
    println!("1. Implement INotificationActivationCallback COM interface");
    println!("2. Register COM server in Windows registry");
    println!("3. Create application shortcut with ToastActivatorCLSID property");
    println!("4. Use consistent AUMID across all components");
    println!();
    println!("This is a Windows platform requirement, not a bug in the library.");
    
    Ok(())
} 