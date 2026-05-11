export interface Env {
  ASSETS: Fetcher;
  PAIRING_ROOMS: DurableObjectNamespace<PairingRoom>;
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);
    if (url.pathname.startsWith('/relay/')) {
      const room = url.pathname.split('/')[2];
      if (!room || !/^[a-zA-Z0-9_-]{6,64}$/.test(room)) {
        return Response.json({ error: 'invalid room' }, { status: 400 });
      }
      const id = env.PAIRING_ROOMS.idFromName(room);
      return env.PAIRING_ROOMS.get(id).fetch(request);
    }
    return env.ASSETS.fetch(request);
  }
};

export class PairingRoom implements DurableObject {
  private sessions = new Set<WebSocket>();

  constructor(_state: DurableObjectState, _env: Env) {}

  async fetch(request: Request): Promise<Response> {
    if (request.headers.get('upgrade') !== 'websocket') {
      return Response.json({ error: 'websocket required' }, { status: 426 });
    }

    const pair = new WebSocketPair();
    const client = pair[0];
    const server = pair[1];
    server.accept();
    this.sessions.add(server);

    server.addEventListener('message', (event) => {
      if (typeof event.data !== 'string' || event.data.length > 16_384) return;
      for (const peer of this.sessions) {
        if (peer !== server) peer.send(event.data);
      }
    });

    const cleanup = () => this.sessions.delete(server);
    server.addEventListener('close', cleanup);
    server.addEventListener('error', cleanup);

    return new Response(null, { status: 101, webSocket: client });
  }
}
