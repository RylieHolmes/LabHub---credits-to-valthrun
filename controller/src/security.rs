// controller/src/security.rs

use windows::Win32::System::Diagnostics::Debug::{IsDebuggerPresent, CheckRemoteDebuggerPresent};
use windows::Win32::System::Threading::GetCurrentProcess;
use windows::Win32::Foundation::BOOL;
use std::time::Instant;

/// Runs a battery of anti-debug checks.
/// If any check fails, the process self-terminates.
pub fn fortify_process() {
    unsafe {
        // 1. PEB Flag Check
        // Checks the Process Environment Block for the "BeingDebugged" flag.
        if IsDebuggerPresent().as_bool() {
            eprintln!("Err: 0x1");
            std::process::exit(0);
        }

        // 2. Remote Debugger Check
        // Checks if another process is attached as a debugger.
        let mut is_remote_debugger = BOOL(0);
        if CheckRemoteDebuggerPresent(GetCurrentProcess(), &mut is_remote_debugger).is_ok() {
            if is_remote_debugger.as_bool() {
                eprintln!("Err: 0x2");
                std::process::exit(0);
            }
        }
        
        // 3. Timing Check (RDTSC equivalent)
        // Detects if code execution is being slowed down (singlesteping).
        let start = Instant::now();
        // Perform a trivial calculation
        let mut x = 0;
        for i in 0..1000 { x += i; }
        std::hint::black_box(x); // Prevent optimization
        let elapsed = start.elapsed();
        
        // If 1000 additions take more than 10ms, someone is stepping through the code.
        if elapsed.as_millis() > 10 {
             std::process::exit(0);
        }
    }
}