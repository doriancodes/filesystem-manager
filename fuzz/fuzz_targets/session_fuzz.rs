#![no_main]
use libfuzzer_sys::fuzz_target;
use froggr::modules::session::SessionManager;
use std::path::PathBuf;

fuzz_target!(|data: &[u8]| {
    // Convert fuzzer data into a path string
    if let Ok(path_str) = std::str::from_utf8(data) {
        // Basic path sanitization
        let path = path_str.replace("..", "").replace(";", "");
        let path = PathBuf::from(format!("/tmp/fuzz_{}", path));
        
        // Create session manager
        if let Ok(session_manager) = SessionManager::new() {
            // Try to create a session
            if let Ok(session_id) = session_manager.create_session(path.clone()) {
                // Try various operations
                let _ = session_manager.get_session(&session_id);
                let _ = session_manager.kill_session(&session_id);
            }
            
            // Try listing sessions
            let _ = session_manager.list_sessions();
            
            // Try purging sessions
            let _ = session_manager.purge_sessions();
        }
    }
}); 