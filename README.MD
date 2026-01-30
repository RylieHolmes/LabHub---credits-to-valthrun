# <span style="color: #CCCCCC">L</span><span style="color: #B3B3B3">A</span><span style="color: #999999">B</span><span style="color: #808080">H</span><span style="color: #666666">u</span><span style="color: #4D4D4D">b</span>

<p align="right">
<a href="https://discord.gg/9eP3VX5f">
<img src="https://img.shields.io/badge/Discord-Join%20Server-7289DA?style=for-the-badge&logo=discord" alt="Join Discord">
</a>
</p>

LabHub is an open source external Counter-Strike 2 read only kernel-level gameplay enhancer.  
That's a lot of descriptive words, but what does each of them mean?  
- `LabHub` The name of this project
- `open source` This application is open source and for everyone to learn from
- `external` We do not inject any DLLs into the target process
- `read only` We do not write to the CS2 process in any way, therefore being impossible to detect by scanning the process memory
- `kernel` We do not use any user level WinAPIs in order to get information from the CS2 process
  
This project is mainly a fun example for exploring the Windows Kernel with [Rust](https://www.rust-lang.org) and exploring the world of game enhancements :)

# WARNING
LabHub is **not** a plug 'n play solution.  
Instead LabHub aims to achieve maximum stealth and stay invisible for other applications.  
Please read [How to use](https://wiki.valth.run/getting-started/) carefully and try troubleshooting issues on your own.  

  
# Features
Due to LabHub being read-only (as of now), there are limitations on what features are possible to implement (eg. skin changer).
Regardless of this limitation, LabHub supports the following features:  

- **Legit Aim**
  - Smooth aim assistance with configurable FOV and smoothing
  - Bone selection (Head, Neck, Spine, Pelvis)
  - Activation key binding
- **Visuals / ESP**  
  - External radar (Publish your game for everybody on https://radar.valth.run)
  - Player ESP with configuration possibilities like: `Skeleton`, `Boxes3D` and `Boxes2D` 
  - Configurable colors to distinguish between enemy and team players
  - ESP includes player health, a health bar, weapon and a lot more
- **World**
  - **Grenade Trajectory**: Helper to see grenade paths
  - **Bomb Info**: Time until detonation, defuse timer, and bomb site location
- **Trigger Bot**  
  - Shoot your enemy (or team mates) as soon they enter your crosshair
  - Configurable delays and burst duration
  - Team check and Recoil Helper
- **Misc**
  - **Sniper Crosshair**: Custom crosshair for snipers with size, thickness, and color customization
  - **Spectator List**: See who is currently watching you
  - **Stream Proof**: Never show your LabHub overlay on any screen shares or captures
  - **Watermark**: Optional overlay watermark

To access LabHub settings overlay press `PAUSE`.

# How to use / Getting started
Please follow this guide to get started:  
https://wiki.valth.run/getting-started/

# Troubleshooting
If you are having issues mapping the kernel driver or starting the controller, please take a look [here](https://wiki.valth.run/category/troubleshooting):  
https://wiki.valth.run/category/troubleshooting
  
The issue you encounter is most likely covered. If you still have any questions or need assistance, feel free to checkout the official LabHub Discord server ([click to join](https://discord.gg/9eP3VX5f)).

# VAC
The same considerations as mentioned in [this link](https://github.com/dretax/GarHal_CSGO#starting-driver) have been taken into account.  
With these precautions and some minor improvements, such as omitting the LabHub identifier and using xor encryption for strings, the driver/overlay should avoid VAC detection. However, I must clarify that I haven't extensively studied VAC, so my conclusion is speculative. Personally, I have been using a C based driver/overlay like this with CSGO for several years without ever getting VAC banned. But be aware of overwatch!  
With VAC live being enabled now, use this with caution. As always take the necessary precautions into consideration.

# Screenshots
![](./img/showcase_01.png)
![](./img/showcase_02.png)
![](./img/showcase_03.png)

# Help
You can find help on the official LabHub Discord server:  
[![Discord Shield](https://img.shields.io/badge/Discord-Join%20Server-7289DA?style=flat&logo=discord)](https://discord.gg/9eP3VX5f)
