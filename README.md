# Minecraft-Chat-Client - Low-Level Rust Minecraft Client

This is a custom, low-level Minecraft client written entirely in Rust. It connects directly to a Minecraft server via TCP, manually decodes the protocol, and processes raw packets. 

While the feature set is currently barebones, the core networking is very good. It serves as a proof-of-concept for interacting with the Minecraft protocol without relying on heavy third-party libraries.

## Core Features

* **Persistent Connection:** The client automatically reads and responds to Keep-Alive (0x1F) packets from the server to maintain an active session.
* **Terminal Chat with Colors:** It receives chat packets, parses the JSON payload, and translates Minecraft's text formatting into ANSI escape sequences. The chat is fully readable and colored right in your terminal.
* **Dynamic Zlib Compression:** Fully supports server-side compression. If the server enables compression, the client automatically catches the threshold and routes subsequent packets through a Zlib decoder/encoder.
* **Server Ping & Icon Extractor:** Before logging in, it sends a status request to fetch the MOTD and player count. It also intercepts the Base64 server favicon and saves it locally as server-icon.png.
* **Interactive CLI:** You can send chat messages or execute server commands directly from your terminal. It also includes local commands (e.g., .list to view online players, .quit to exit).

## Known Limitations & Warnings

The project has a few hard limitations you need to be aware of:

* **Locked to 1.16:** The protocol version is currently hardcoded to 754. This means the client will only work with Minecraft 1.16 servers.
* **No Player Physics:** The client currently only handles networking and chat. It does not send any position, rotation, or gravity updates. I do not recommend doing this on public servers with strict Anti-Cheat plugins. Since your character is essentially floating in the void without sending movement packets, you will most likely get automatically kicked or banned.
