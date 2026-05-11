# Remote Web Control

The Cloudflare Worker deployment hosts the same GUI as the local daemon and provides `/relay/:room` WebSocket rooms.

The relay is control-plane only. Browser and local daemon messages must be end-to-end encrypted before they enter the Worker:

1. Browser creates a pairing room and displays a short code.
2. Local daemon connects to the same room after the user enters the code.
3. Both sides exchange P-256 public keys and derive an AES-GCM session key.
4. Only encrypted JSON control messages cross the Worker.
5. Audio files, decoded PCM, and conversion outputs never cross Cloudflare.

The current Worker scaffold validates room ids and broadcasts bounded string messages. The pairing cryptography is intentionally kept in the client/daemon layer so the Worker cannot inspect payloads.

