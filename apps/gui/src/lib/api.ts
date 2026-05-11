export type ToolInfo = {
  name: string;
  path?: string | null;
  version?: string | null;
  available: boolean;
};

export type CodecCapabilities = {
  ffmpeg: ToolInfo;
  ffprobe: ToolInfo;
  mpv: ToolInfo;
  decoders: string[];
  encoders: string[];
  muxers: string[];
  demuxers: string[];
  android_codec_pack: string;
};

export type Health = {
  app: string;
  version: string;
  database_path: string;
  capabilities: CodecCapabilities;
};

export type Track = {
  id: number;
  path: string;
  title: string;
  artist: string;
  album: string;
  duration_ms?: number | null;
  format?: string | null;
  codec?: string | null;
};

export type Playlist = {
  id: number;
  name: string;
  track_count: number;
  created_at: string;
  updated_at: string;
};

export type PlaylistDetail = {
  playlist: Playlist;
  tracks: Track[];
};

export type PlaybackState = {
  loaded_path?: string | null;
  playing: boolean;
  position_seconds: number;
  volume: number;
};

export type JobRecord = {
  id: string;
  kind: string;
  state: string;
  source_path?: string | null;
  output_path?: string | null;
  progress: number;
  message?: string | null;
};

export type AuraluxEvent =
  | { type: 'library_scan_progress'; scanned: number; imported: number; skipped: number; current_path?: string | null }
  | { type: 'library_updated' }
  | { type: 'playlist_updated'; playlist_id: number }
  | { type: 'playback_state'; state: PlaybackState }
  | { type: 'job_progress'; job: JobRecord }
  | { type: 'error'; message: string };

const apiBase = import.meta.env.VITE_AURALUX_API_BASE ?? '';

export async function apiGet<T>(path: string): Promise<T> {
  const response = await fetch(`${apiBase}${path}`);
  if (!response.ok) throw new Error(await errorText(response));
  return response.json();
}

export async function apiPost<T>(path: string, body: unknown): Promise<T> {
  const response = await fetch(`${apiBase}${path}`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body)
  });
  if (!response.ok) throw new Error(await errorText(response));
  return response.json();
}

export async function apiUpload<T>(path: string, body: FormData): Promise<T> {
  const response = await fetch(`${apiBase}${path}`, {
    method: 'POST',
    body
  });
  if (!response.ok) throw new Error(await errorText(response));
  return response.json();
}

export function eventUrl(): string {
  const base = apiBase || window.location.origin;
  const url = new URL('/api/events', base);
  url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
  return url.toString();
}

async function errorText(response: Response): Promise<string> {
  try {
    const body = await response.json();
    return body.error ?? response.statusText;
  } catch {
    return response.statusText;
  }
}
