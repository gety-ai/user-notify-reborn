use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tokio::runtime::Runtime;
use user_notify_reborn::prelude::*;

#[test]
fn test_notification_from_non_main_thread() {
    let _ = env_logger::try_init();

    // use Arc<Mutex<>> to share results between threads
    let result = Arc::new(Mutex::new(None));
    let result_clone = Arc::clone(&result);

    // run notification test in non-main thread
    let handle = thread::spawn(move || {
        // create tokio runtime in new thread
        let rt = Runtime::new().expect("Failed to create tokio runtime");

        let test_result = rt.block_on(async {
            // create notification manager
            let manager =
                match NotifyManager::try_new("com.example.user-notify-test", Some("test-notify")) {
                    Ok(m) => m,
                    Err(e) => {
                        eprintln!("Failed to create notification manager: {e}");
                        return false;
                    }
                };

            // register notification handler
            let notification_received = Arc::new(Mutex::new(false));
            let notification_received_clone = Arc::clone(&notification_received);

            if let Err(e) = manager.register(
                Box::new(move |response| {
                    println!("Notification response received in thread: {response:?}");
                    *notification_received_clone.lock().unwrap() = true;
                }),
                vec![],
            ) {
                eprintln!("Failed to register notification handler: {e}");
                return false;
            }

            // request notification permission
            match manager.first_time_ask_for_notification_permission().await {
                Ok(permission) => {
                    println!("Notification permission in thread: {permission}");
                    if !permission {
                        println!(
                            "Warning: Notification permission not granted, but continuing test"
                        );
                    }
                }
                Err(e) => {
                    eprintln!("Failed to request notification permission: {e}");
                    // on some platforms this may fail, but we continue testing
                }
            }

            // create and send notification
            let notification = NotifyBuilder::new()
                .title("Test from Non-Main Thread")
                .body("This notification was sent from a non-main thread")
                .subtitle("Thread Test")
                .sound("default");

            match manager.send(notification).await {
                Ok(handle) => {
                    println!(
                        "Notification sent successfully from thread with ID: {}",
                        handle.get_id()
                    );

                    // wait for a while to process notification
                    tokio::time::sleep(Duration::from_secs(2)).await;

                    // check active notifications
                    match manager.get_active_notifications().await {
                        Ok(active) => {
                            println!("Active notifications in thread: {}", active.len());
                        }
                        Err(e) => {
                            eprintln!("Failed to get active notifications: {e}");
                        }
                    }

                    // clean up notifications
                    // if let Err(e) = manager.remove_all_delivered_notifications() {
                    //     eprintln!("Failed to remove notifications: {}", e);
                    // }

                    true
                }
                Err(e) => {
                    eprintln!("Failed to send notification from thread: {e}");
                    false
                }
            }
        });

        *result_clone.lock().unwrap() = Some(test_result);
    });

    // wait for thread to complete
    handle.join().expect("Thread panicked");

    // check results
    let final_result = result.lock().unwrap().expect("No result set");
    assert!(final_result, "Notification test in non-main thread failed");

    println!("✅ Non-main thread notification test completed successfully");
}

#[test]
fn test_multiple_threads_concurrent_notifications() {
    let _ = env_logger::try_init();

    // test multiple threads sending notifications concurrently
    let mut handles = vec![];
    let results = Arc::new(Mutex::new(Vec::new()));

    for i in 0..3 {
        let results_clone = Arc::clone(&results);
        let handle = thread::spawn(move || {
            let rt = Runtime::new().expect("Failed to create tokio runtime");

            let result = rt.block_on(async {
                let manager = match NotifyManager::try_new(
                    &format!("com.example.thread-test-{i}"),
                    Some(&format!("thread-test-{i}")),
                ) {
                    Ok(m) => m,
                    Err(e) => {
                        eprintln!("Thread {i}: Failed to create manager: {e}");
                        return false;
                    }
                };

                // register handler
                if let Err(e) = manager.register(
                    Box::new(move |response| {
                        println!("Thread {i}: Received response: {response:?}");
                    }),
                    vec![],
                ) {
                    eprintln!("Thread {i}: Failed to register handler: {e}");
                    return false;
                }

                // send notification
                let notification = NotifyBuilder::new()
                    .title(&format!("Thread {i} Notification"))
                    .body(&format!("This is from thread number {i}"))
                    .subtitle("Concurrent Test");

                match manager.send(notification).await {
                    Ok(handle) => {
                        println!(
                            "Thread {}: Notification sent with ID: {}",
                            i,
                            handle.get_id()
                        );

                        // wait for a while
                        tokio::time::sleep(Duration::from_millis(500)).await;

                        // clean up
                        // let _ = manager.remove_all_delivered_notifications();

                        true
                    }
                    Err(e) => {
                        eprintln!("Thread {i}: Failed to send notification: {e}");
                        false
                    }
                }
            });

            results_clone.lock().unwrap().push(result);
        });

        handles.push(handle);
    }

    // wait for all threads to complete
    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // check all results
    let final_results = results.lock().unwrap();
    assert_eq!(final_results.len(), 3, "Not all threads completed");

    for (i, &result) in final_results.iter().enumerate() {
        assert!(result, "Thread {i} failed");
    }

    println!("✅ Concurrent multi-thread notification test completed successfully");
}

#[tokio::test]
async fn test_async_spawn_notification() {
    let _ = env_logger::try_init();

    // test sending notifications in tokio::spawn
    let manager = NotifyManager::try_new("com.example.async-spawn-test", Some("async-spawn-test"))
        .expect("Failed to create notification manager");

    // register handler
    manager
        .register(
            Box::new(|response| {
                println!("Async spawn: Received response: {response:?}");
            }),
            vec![],
        )
        .expect("Failed to register handler");

    // send notification in async task
    let task_result = tokio::spawn(async move {
        let notification = NotifyBuilder::new()
            .title("Async Spawn Test")
            .body("This notification was sent from tokio::spawn")
            .subtitle("Async Task");

        match manager.send(notification).await {
            Ok(handle) => {
                println!(
                    "Async spawn: Notification sent with ID: {}",
                    handle.get_id()
                );

                // wait for a while
                tokio::time::sleep(Duration::from_millis(1000)).await;

                // clean up
                // let _ = manager.remove_all_delivered_notifications();

                true
            }
            Err(e) => {
                eprintln!("Async spawn: Failed to send notification: {e}");
                false
            }
        }
    })
    .await;

    let result = task_result.expect("Async task panicked");
    assert!(result, "Async spawn notification test failed");

    println!("✅ Async spawn notification test completed successfully");
}
