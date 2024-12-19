#![no_main]
use libfuzzer_sys::fuzz_target;
use froggr::modules::session::{SessionManager, SessionCommand};
use std::path::PathBuf;

fuzz_target!(|data: &[u8]| {
    if let Ok(command_str) = std::str::from_utf8(data) {
        // Try to parse as command
        if let Ok(command) = serde_json::from_str::<SessionCommand>(command_str) {
            if let Ok(session_manager) = SessionManager::new() {
                // Create a test session
                let test_path = PathBuf::from("/tmp/fuzz_test");
                if let Ok(session_id) = session_manager.create_session(test_path) {
                    match command {
                        SessionCommand::Mount { source, target, node_id } => {
                            let _ = session_manager.send_mount_command(
                                &session_id,
                                source,
                                target,
                                node_id
                            );
                        },
                        SessionCommand::Bind { source, target, mode } => {
                            // Handle bind command
                            let _ = session_manager.send_bind_command(
                                &session_id,
                                source,
                                target,
                                mode
                            );
                        }
                    }
                }
            }
        }
    }
}); 