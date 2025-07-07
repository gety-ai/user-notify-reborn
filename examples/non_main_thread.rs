use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use user_notify_reborn::prelude::*;

const DEFAULT_BUNDLE_ID: &str = "com.example.user-notify-reborn";

fn get_test_bundle_id() -> String {
    std::env::var("TEST_BUNDLE_ID").unwrap_or_else(|_| DEFAULT_BUNDLE_ID.to_string())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    println!("🚀 Starting Tauri-style notification example...");
    println!("📋 Setup: Main thread handles initialization, other threads send notifications");

    // Test 1: Tauri-style setup with single worker thread
    println!("\n📋 Test 1: Tauri-style setup with single worker thread");
    test_tauri_style_single_thread().await?;

    // Test 2: Tauri-style setup with multiple worker threads
    println!("\n📋 Test 2: Tauri-style setup with multiple worker threads");
    test_tauri_style_multiple_threads().await?;

    // Test 3: Tauri-style with async operations in threads
    println!("\n📋 Test 3: Tauri-style with async operations in threads");
    test_tauri_style_async_threads().await?;

    println!("\n✅ All Tauri-style tests completed successfully!");

    // Show platform-specific tips
    #[cfg(target_os = "macos")]
    print_macos_tips();

    #[cfg(target_os = "windows")]
    print_windows_tips();

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    print_platform_tips();

    Ok(())
}

async fn test_tauri_style_single_thread() -> Result<(), Box<dyn std::error::Error>> {
    // Main thread: Setup notification manager
    println!("🔧 Main thread: Setting up notification manager...");

    // Create notification manager on main thread
    let manager = match NotifyManager::try_new(&get_test_bundle_id(), Some("tauri-style")) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("❌ Main thread: Failed to create notification manager: {e}");
            return Ok(());
        }
    };

    // Register notification handler on main thread
    if let Err(e) = manager.register(
        Box::new(|response| {
            println!("📬 Main thread: Notification response received: {response:?}");
        }),
        vec![],
    ) {
        eprintln!("❌ Main thread: Failed to register notification handler: {e}");
        return Ok(());
    }

    // Request notification permission on main thread
    match manager.first_time_ask_for_notification_permission().await {
        Ok(permission) => {
            println!("🔐 Main thread: Notification permission: {permission}");
            if !permission {
                println!("⚠️  Warning: Notification permission not granted, but continuing test");
            }
        }
        Err(e) => {
            eprintln!("❌ Main thread: Failed to request notification permission: {e}");
            #[cfg(target_os = "macos")]
            {
                eprintln!("💡 On macOS, make sure your app has a proper bundle identifier");
            }
        }
    }

    println!("✅ Main thread: Setup completed");

    // Worker thread: Send notifications
    println!("🧵 Spawning worker thread to send notifications...");

    let result = Arc::new(Mutex::new(false));
    let result_clone = Arc::clone(&result);

    // Use a channel to communicate between threads instead of sharing the manager
    let (tx, rx) = tokio::sync::oneshot::channel();
    let manager_clone = manager.clone();

    let handle = thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new()
            .expect("Failed to create tokio runtime in worker thread");

        let success = rt.block_on(async {
            println!("📤 Worker thread: Sending notification...");

            let notification = NotifyBuilder::new()
                .title("Tauri-Style Test")
                .body("This notification was sent from a worker thread!")
                .subtitle("Setup on main, send on worker")
                .sound("default");

            match manager_clone.send(notification).await {
                Ok(handle) => {
                    println!(
                        "✅ Worker thread: Notification sent successfully with ID: {}",
                        handle.get_id()
                    );

                    // Wait for notification to be processed
                    tokio::time::sleep(Duration::from_secs(2)).await;

                    // Check active notifications
                    match manager_clone.get_active_notifications().await {
                        Ok(active) => {
                            println!("📊 Worker thread: Active notifications: {}", active.len());
                        }
                        Err(e) => {
                            eprintln!("⚠️  Worker thread: Failed to get active notifications: {e}");
                        }
                    }

                    true
                }
                Err(e) => {
                    eprintln!("❌ Worker thread: Failed to send notification: {e}");
                    false
                }
            }
        });

        *result_clone.lock().unwrap() = success;
        // Send completion signal
        let _ = tx.send(());
    });

    // Wait for worker thread to complete
    let _ = rx.await;
    handle.join().expect("Worker thread panicked");

    // Check results
    let final_result = *result.lock().unwrap();
    if final_result {
        println!("✅ Tauri-style single thread test passed");
    } else {
        println!("❌ Tauri-style single thread test failed");
    }

    // Keep main thread alive briefly to see notifications
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Keep manager alive until the end of the function
    drop(manager);

    Ok(())
}

async fn test_tauri_style_multiple_threads() -> Result<(), Box<dyn std::error::Error>> {
    // Main thread: Setup notification manager
    println!("🔧 Main thread: Setting up notification manager for multiple workers...");

    let manager = match NotifyManager::try_new(&get_test_bundle_id(), Some("tauri-multi")) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("❌ Main thread: Failed to create notification manager: {e}");
            return Ok(());
        }
    };

    // Register handler with worker identification
    if let Err(e) = manager.register(
        Box::new(|response| {
            println!("📬 Main thread: Response from worker notification: {response:?}");
        }),
        vec![],
    ) {
        eprintln!("❌ Main thread: Failed to register handler: {e}");
        return Ok(());
    }

    println!("✅ Main thread: Multi-worker setup completed");

    // Spawn multiple worker threads
    let mut handles = vec![];
    let results = Arc::new(Mutex::new(Vec::new()));
    let mut completion_receivers = vec![];

    println!("🧵 Spawning 3 worker threads...");
    for worker_id in 0..3 {
        let results_clone = Arc::clone(&results);
        let manager_clone = manager.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        completion_receivers.push(rx);

        let handle = thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new()
                .expect("Failed to create tokio runtime in worker thread");

            let result = rt.block_on(async {
                println!("📤 Worker {worker_id}: Preparing notification...");

                // Add a small delay to simulate different timing
                tokio::time::sleep(Duration::from_millis(worker_id * 500)).await;

                let notification = NotifyBuilder::new()
                    .title(&format!("Worker {worker_id} Notification"))
                    .body(&format!(
                        "This notification was sent from worker thread #{worker_id}"
                    ))
                    .subtitle("Multi-Worker Test")
                    .set_thread_id(&format!("worker-{worker_id}"));

                match manager_clone.send(notification).await {
                    Ok(handle) => {
                        println!(
                            "✅ Worker {}: Notification sent with ID: {}",
                            worker_id,
                            handle.get_id()
                        );

                        // Brief wait
                        tokio::time::sleep(Duration::from_millis(1000)).await;
                        true
                    }
                    Err(e) => {
                        eprintln!("❌ Worker {worker_id}: Failed to send notification: {e}");
                        false
                    }
                }
            });

            results_clone.lock().unwrap().push((worker_id, result));
            // Send completion signal
            let _ = tx.send(());
        });

        handles.push(handle);
    }

    // Wait for all worker threads to complete
    for rx in completion_receivers {
        let _ = rx.await;
    }

    for handle in handles {
        handle.join().expect("Worker thread panicked");
    }

    // Check all results
    let final_results = results.lock().unwrap();
    let success_count = final_results.iter().filter(|(_, result)| *result).count();

    println!(
        "📊 Multi-worker test completed: {}/{} workers succeeded",
        success_count,
        final_results.len()
    );

    for (worker_id, result) in final_results.iter() {
        if *result {
            println!("✅ Worker {worker_id} passed");
        } else {
            println!("❌ Worker {worker_id} failed");
        }
    }

    // Keep main thread alive briefly to see notifications
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Keep manager alive until the end of the function
    drop(manager);

    Ok(())
}

async fn test_tauri_style_async_threads() -> Result<(), Box<dyn std::error::Error>> {
    // Main thread: Setup
    println!("🔧 Main thread: Setting up for async worker operations...");

    let manager = match NotifyManager::try_new(&get_test_bundle_id(), Some("tauri-async")) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("❌ Main thread: Failed to create notification manager: {e}");
            return Ok(());
        }
    };

    if let Err(e) = manager.register(
        Box::new(|response| {
            println!("📬 Main thread: Async worker response: {response:?}");
        }),
        vec![],
    ) {
        eprintln!("❌ Main thread: Failed to register handler: {e}");
        return Ok(());
    }

    println!("✅ Main thread: Async worker setup completed");

    // Worker thread with nested async operations
    let result = Arc::new(Mutex::new(false));
    let result_clone = Arc::clone(&result);
    let manager_clone = manager.clone();
    let (tx, rx) = tokio::sync::oneshot::channel();

    println!("🧵 Spawning worker thread with nested async operations...");

    let handle = thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new()
            .expect("Failed to create tokio runtime in worker thread");

        let success = rt.block_on(async {
            println!("📤 Worker thread: Starting async notification sequence...");

            // Simulate multiple async operations in sequence
            let tasks = [
                ("First", "First async notification from worker thread"),
                ("Second", "Second async notification with delay"),
                ("Final", "Final notification in the sequence"),
            ];

            let mut all_success = true;

            for (i, (name, body)) in tasks.iter().enumerate() {
                println!("📤 Worker thread: Sending {name} notification...");

                let notification = NotifyBuilder::new()
                    .title(&format!("Async Worker - {name}"))
                    .body(body)
                    .subtitle("Async Sequence Test")
                    .set_thread_id("async-sequence");

                match manager_clone.send(notification).await {
                    Ok(handle) => {
                        println!(
                            "✅ Worker thread: {} notification sent with ID: {}",
                            name,
                            handle.get_id()
                        );
                    }
                    Err(e) => {
                        eprintln!("❌ Worker thread: Failed to send {name} notification: {e}");
                        all_success = false;
                    }
                }

                // Wait between notifications
                if i < tasks.len() - 1 {
                    tokio::time::sleep(Duration::from_millis(1500)).await;
                }
            }

            // Final check
            tokio::time::sleep(Duration::from_secs(1)).await;

            match manager_clone.get_active_notifications().await {
                Ok(active) => {
                    println!(
                        "📊 Worker thread: Final active notifications: {}",
                        active.len()
                    );
                }
                Err(e) => {
                    eprintln!("⚠️  Worker thread: Failed to get final active notifications: {e}");
                }
            }

            all_success
        });

        *result_clone.lock().unwrap() = success;
        // Send completion signal
        let _ = tx.send(());
    });

    // Wait for worker thread to complete
    let _ = rx.await;
    handle.join().expect("Worker thread panicked");

    // Check results
    let final_result = *result.lock().unwrap();
    if final_result {
        println!("✅ Tauri-style async thread test passed");
    } else {
        println!("❌ Tauri-style async thread test failed");
    }

    // Keep main thread alive briefly to see notifications
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Keep manager alive until the end of the function
    drop(manager);

    Ok(())
}

#[cfg(target_os = "macos")]
fn print_macos_tips() {
    println!("\n🍎 macOS Tips for Tauri-style usage:");
    println!("  • Make sure your main app has a proper bundle identifier");
    println!("  • The NotifyManager should be created once on the main thread");
    println!("  • Worker threads can safely send notifications using the shared manager");
    println!("  • For testing: cp examples/Info.plist target/debug/");
    println!("  • Or build and sign: bash examples/build_and_sign.sh");
}

#[cfg(target_os = "windows")]
fn print_windows_tips() {
    println!("\n🪟 Windows Tips for Tauri-style usage:");
    println!("  • Register your app with the system once on the main thread");
    println!("  • The app_id should match your application's identifier");
    println!("  • Worker threads can safely use the shared NotifyManager");
    println!("  • Ensure proper app registration for best results");
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn print_platform_tips() {
    println!("\n🐧 Platform Info:");
    println!("  • This platform is not currently supported");
    println!("  • Supported platforms: macOS, Windows");
    println!("  • Tauri-style pattern: setup on main thread, send from workers");
}
