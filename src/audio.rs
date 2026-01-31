//! Audible alert support for accessibility.
//!
//! Provides sound notifications for price alerts using system beep.

/// Sound different patterns for different alert types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertSound {
    /// Single beep for regular alert
    Single,
    /// Double beep for high priority
    Double,
    /// Triple beep for critical alert
    Triple,
}

/// Play an audible alert using system beep.
/// On Unix/Linux/macOS: Uses BEL character (\x07)
/// On Windows: Uses system beep
pub fn play_sound(sound: AlertSound) {
    let beep_count = match sound {
        AlertSound::Single => 1,
        AlertSound::Double => 2,
        AlertSound::Triple => 3,
    };

    // Use the BEL character for simple cross-platform beeping
    // This works in most terminal emulators
    for _ in 0..beep_count {
        print!("\x07"); // BEL character
        use std::io::Write;
        let _ = std::io::stdout().flush();
        
        // Small delay between beeps
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

/// Play alert sound with delay (non-blocking version for async contexts)
/// Returns immediately but schedules the beep
pub fn play_sound_async(sound: AlertSound) {
    // Spawn a thread to avoid blocking the UI thread
    std::thread::spawn(move || {
        play_sound(sound);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alert_sound_variants() {
        // Just verify the enum variants exist and can be used
        let _single = AlertSound::Single;
        let _double = AlertSound::Double;
        let _triple = AlertSound::Triple;
    }

    #[test]
    fn test_alert_sound_equality() {
        assert_eq!(AlertSound::Single, AlertSound::Single);
        assert_ne!(AlertSound::Single, AlertSound::Double);
    }
}
