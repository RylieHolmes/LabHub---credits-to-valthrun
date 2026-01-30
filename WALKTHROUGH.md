# Valthrun Custom Controller - Walkthrough

## Overview
This is a customized version of the Valthrun CS2 external cheat controller. It has been modified to remove authentication, remove Discord integration, use a stable DirectX overlay, and includes critical fixes for memory reading crashes.

## Key Changes & Fixes
- **Authentication Removed**: Logic for verifying license keys has been stripped out.
- **Discord RPC Removed**: Privacy focused; no Discord connection logic.
- **Crash Fix (ClassNameCache)**: Fixed a critical offset error (`0x30` -> `0x08`) in `cs2/src/class_name_cache.rs` that was causing the cheat to read invalid memory, crash-loop, and flicker.
- **Overlay Stability**:
    - Reverted to standard `MoveWindow` logic (instead of `SetWindowPos`) for better compatibility.
    - Switched default renderer to **DirectX** for better performance and icon support.
    - Verified "1-pixel shrink" hack is active to keep the overlay topmost.

## How to Build
To build the controller from source:

1.  Open a terminal in the `cs2-main` directory.
2.  Run the build command:
    ```powershell
    cargo build --release --bin controller
    ```
3.  The output binary will be at `target/release/controller.exe`.

## How to Run (Recommended)
For best results and to avoid file locking issues:

1.  Copy the built executable to a separate name/location (e.g., `debug_controller.exe`).
    ```powershell
    copy target\release\controller.exe debug_controller.exe
    ```
2.  **Run CS2** in Borderless Windowed or Windowed mode.
3.  **Run `debug_controller.exe`** as Administrator.

## Troubleshooting
- **Flickering**: If flickering returns, ensure you are in Borderless Windowed mode.
- **Missing ESP**: If ESP boxes disappear after a game update, check `cs2/src/class_name_cache.rs` offsets or `cs2-schema`.
- **Overlay Not Appearing**: Check if `OverlayTarget` in `controller/src/main.rs` matches the CS2 window class/name.

## Project Structure
- `controller/`: Main application logic and features.
- `cs2/`: Library for interacting with CS2 memory and schemas.
- `overlay/`: Rendering engine (ImGui + DirectX/OpenGL).
- `cs2-schema/`: Generated schema files for CS2 offsets.
