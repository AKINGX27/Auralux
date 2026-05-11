<script lang="ts">
  import {
    Activity,
    Album,
    FolderPlus,
    Library,
    ListMusic,
    Pause,
    Play,
    RefreshCw,
    Search,
    Settings,
    Shuffle,
    SlidersHorizontal,
    Sparkles
  } from 'lucide-svelte';
  import { onMount } from 'svelte';
  import CapabilityList from './lib/CapabilityList.svelte';
  import {
    apiGet,
    apiPost,
    apiUpload,
    eventUrl,
    type AuraluxEvent,
    type Health,
    type JobRecord,
    type Playlist,
    type PlaylistDetail,
    type PlaybackState,
    type Track
  } from './lib/api';
  import { formatDuration, pct } from './lib/format';

  let view: 'library' | 'playlists' | 'conversions' | 'settings' = 'library';
  let health: Health | null = null;
  let tracks: Track[] = [];
  let playlists: Playlist[] = [];
  let activePlaylist: PlaylistDetail | null = null;
  let jobs: JobRecord[] = [];
  let playback: PlaybackState = { playing: false, position_seconds: 0, volume: 100 };
  let search = '';
  let scanPath = '';
  let convertSource = '';
  let convertOutput = '';
  let convertFormat = 'opus';
  let busy = false;
  let error = '';
  let status = 'Local core not connected';
  let playlistDropActive = false;
  let playlistImporting = false;

  const nav = [
    { id: 'library', label: 'Library', icon: Library },
    { id: 'playlists', label: 'Playlists', icon: ListMusic },
    { id: 'conversions', label: 'Convert', icon: SlidersHorizontal },
    { id: 'settings', label: 'Settings', icon: Settings }
  ] as const;

  onMount(() => {
    void refresh();
    connectEvents();
  });

  async function refresh() {
    busy = true;
    error = '';
    try {
      [health, tracks, jobs, playlists] = await Promise.all([
        apiGet<Health>('/api/health'),
        apiGet<Track[]>(`/api/library/tracks${search ? `?search=${encodeURIComponent(search)}` : ''}`),
        apiGet<JobRecord[]>('/api/jobs'),
        apiGet<Playlist[]>('/api/playlists')
      ]);
      await syncActivePlaylist();
      playback = await apiGet<PlaybackState>('/api/playback/state').catch(() => playback);
      status = 'Local core connected';
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
      status = 'Local core unavailable';
    } finally {
      busy = false;
    }
  }

  function connectEvents() {
    try {
      const socket = new WebSocket(eventUrl());
      socket.onmessage = (message) => {
        const event = JSON.parse(message.data) as AuraluxEvent;
        if (event.type === 'job_progress') {
          jobs = [event.job, ...jobs.filter((job) => job.id !== event.job.id)].slice(0, 50);
        }
        if (event.type === 'playback_state') playback = event.state;
        if (event.type === 'library_updated') void refresh();
        if (event.type === 'playlist_updated') void refreshPlaylists(event.playlist_id);
        if (event.type === 'library_scan_progress') {
          status = `Scanning ${event.scanned} files, ${event.imported} imported`;
        }
        if (event.type === 'error') error = event.message;
      };
      socket.onclose = () => setTimeout(connectEvents, 2500);
    } catch {
      status = 'Events disconnected';
    }
  }

  async function syncActivePlaylist() {
    if (playlists.length === 0) {
      activePlaylist = null;
      return;
    }
    const current = activePlaylist && playlists.find((playlist) => playlist.id === activePlaylist?.playlist.id);
    const selected = current ?? playlists[0];
    activePlaylist = await apiGet<PlaylistDetail>(`/api/playlists/${selected.id}`);
  }

  async function refreshPlaylists(preferredId?: number) {
    try {
      playlists = await apiGet<Playlist[]>('/api/playlists');
      if (preferredId) {
        activePlaylist = await apiGet<PlaylistDetail>(`/api/playlists/${preferredId}`);
      } else {
        await syncActivePlaylist();
      }
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  function dragTrack(event: DragEvent, track: Track) {
    event.dataTransfer?.setData('application/x-auralux-track-id', String(track.id));
    event.dataTransfer?.setData('text/plain', track.title);
    if (event.dataTransfer) event.dataTransfer.effectAllowed = 'copy';
  }

  function dragOverPlaylist(event: DragEvent) {
    if (!event.dataTransfer) return;
    event.preventDefault();
    event.dataTransfer.dropEffect = 'copy';
    playlistDropActive = true;
  }

  async function dropOnPlaylist(event: DragEvent) {
    event.preventDefault();
    playlistDropActive = false;
    const transfer = event.dataTransfer;
    const droppedFiles = [...(transfer?.files ?? [])].filter(isAudioFile);
    const trackId = Number(transfer?.getData('application/x-auralux-track-id'));
    if (droppedFiles.length === 0 && (!Number.isFinite(trackId) || trackId <= 0)) return;
    try {
      const playlist = activePlaylist?.playlist ?? (await apiPost<Playlist>('/api/playlists', { name: 'Favorites' }));
      if (!activePlaylist || activePlaylist.playlist.id !== playlist.id) {
        activePlaylist = await apiGet<PlaylistDetail>(`/api/playlists/${playlist.id}`);
      }
      if (droppedFiles.length > 0) {
        const form = new FormData();
        for (const file of droppedFiles) form.append('files', file, file.name);
        playlistImporting = true;
        activePlaylist = await apiUpload<PlaylistDetail>(`/api/playlists/${playlist.id}/import`, form);
        tracks = await apiGet<Track[]>(`/api/library/tracks${search ? `?search=${encodeURIComponent(search)}` : ''}`);
      } else if (Number.isFinite(trackId) && trackId > 0) {
        activePlaylist = await apiPost<PlaylistDetail>(`/api/playlists/${playlist.id}/tracks`, { track_id: trackId });
      } else {
        return;
      }
      playlists = await apiGet<Playlist[]>('/api/playlists');
      view = 'playlists';
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      playlistImporting = false;
    }
  }

  function isAudioFile(file: File) {
    if (file.type.startsWith('audio/')) return true;
    const extension = file.name.split('.').pop()?.toLowerCase() ?? '';
    return [
      'aac',
      'aif',
      'aiff',
      'alac',
      'ape',
      'caf',
      'dff',
      'dsf',
      'flac',
      'm4a',
      'mka',
      'mp2',
      'mp3',
      'mp4',
      'mpc',
      'oga',
      'ogg',
      'opus',
      'tak',
      'tta',
      'wav',
      'weba',
      'wma',
      'wv'
    ].includes(extension);
  }

  async function ensureDefaultPlaylist() {
    try {
      const playlist = await apiPost<Playlist>('/api/playlists', { name: 'Favorites' });
      playlists = await apiGet<Playlist[]>('/api/playlists');
      activePlaylist = await apiGet<PlaylistDetail>(`/api/playlists/${playlist.id}`);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  async function selectPlaylist(playlist: Playlist) {
    try {
      activePlaylist = await apiGet<PlaylistDetail>(`/api/playlists/${playlist.id}`);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  async function runScan() {
    if (!scanPath.trim()) return;
    busy = true;
    try {
      await apiPost('/api/library/scan', { roots: [scanPath.trim()], force: false });
      scanPath = '';
      await refresh();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  async function playTrack(track: Track) {
    try {
      playback = await apiPost<PlaybackState>('/api/playback/load', { path: track.path, play: true });
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  async function togglePlayback() {
    const command = playback.playing ? { command: 'pause' } : { command: 'play' };
    try {
      playback = await apiPost<PlaybackState>('/api/playback/command', command);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  async function createConversion() {
    if (!convertSource.trim() || !convertOutput.trim()) return;
    busy = true;
    try {
      const job = await apiPost<JobRecord>('/api/conversions', {
        source_path: convertSource.trim(),
        output_dir: convertOutput.trim(),
        preset: { format: convertFormat, quality: null },
        overwrite: false
      });
      jobs = [job, ...jobs.filter((item) => item.id !== job.id)];
      convertSource = '';
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }
</script>

<div class="shell">
  <aside class="sidebar glass">
    <div class="brand">
      <div class="brand-mark"><Sparkles size={18} /></div>
      <div>
        <strong>Auralux</strong>
        <span>{status}</span>
      </div>
    </div>
    <nav>
      {#each nav as item}
        <button class:active={view === item.id} onclick={() => (view = item.id)} aria-label={item.label}>
          <svelte:component this={item.icon} size={18} />
          <span>{item.label}</span>
        </button>
      {/each}
    </nav>
  </aside>

  <main class="main">
    <header class="topbar glass">
      <div class="search">
        <Search size={17} />
        <input bind:value={search} onkeydown={(event) => event.key === 'Enter' && refresh()} placeholder="Search library" />
      </div>
      <button class="icon-button" onclick={refresh} aria-label="Refresh" title="Refresh">
        <RefreshCw size={18} class={busy ? 'spin' : ''} />
      </button>
    </header>

    {#if error}
      <section class="notice glass">{error}</section>
    {/if}

    {#if view === 'library'}
      <section class="library-grid">
        <div class="panel glass">
          <div class="panel-head">
            <div>
              <p>Library</p>
              <h1>{tracks.length} tracks</h1>
            </div>
            <Album size={24} />
          </div>
          <div class="scan-row">
            <input bind:value={scanPath} placeholder="/Users/name/Music or /sdcard/Music" />
            <button onclick={runScan}><FolderPlus size={17} />Scan</button>
          </div>
          <div class="track-list">
            {#each tracks as track}
              <button class="track" draggable="true" ondragstart={(event) => dragTrack(event, track)} onclick={() => playTrack(track)}>
                <div class="cover">{track.title.slice(0, 1).toUpperCase()}</div>
                <div class="track-meta">
                  <strong>{track.title}</strong>
                  <span>{track.artist} · {track.album}</span>
                </div>
                <span>{track.codec ?? track.format ?? 'audio'}</span>
                <time>{formatDuration(track.duration_ms)}</time>
              </button>
            {:else}
              <div class="empty">No tracks yet.</div>
            {/each}
          </div>
        </div>

        <aside class="queue glass">
          <div class="panel-head">
            <div>
              <p>Queue</p>
              <h2>Up next</h2>
            </div>
            <Shuffle size={21} />
          </div>
          {#each tracks.slice(0, 8) as track}
            <div class="queue-item">
              <span>{track.title}</span>
              <small>{track.artist}</small>
            </div>
          {:else}
            <div class="empty compact">No queued tracks.</div>
          {/each}
        </aside>
      </section>
    {:else if view === 'playlists'}
      <section class="playlist-layout">
        <aside class="playlist-list glass">
          <div class="panel-head">
            <div>
              <p>Playlists</p>
              <h2>{playlists.length} lists</h2>
            </div>
            <button class="icon-button" onclick={ensureDefaultPlaylist} aria-label="New playlist" title="New playlist">
              <ListMusic size={18} />
            </button>
          </div>
          {#each playlists as playlist}
            <button class="playlist-tab" class:active={activePlaylist?.playlist.id === playlist.id} onclick={() => selectPlaylist(playlist)}>
              <strong>{playlist.name}</strong>
              <span>{playlist.track_count} tracks</span>
            </button>
          {:else}
            <button class="playlist-tab active" onclick={ensureDefaultPlaylist}>
              <strong>Favorites</strong>
              <span>Create playlist</span>
            </button>
          {/each}
        </aside>

        <section
          class="panel glass playlist-drop"
          class:drop-active={playlistDropActive}
          aria-label="Playlist drop area"
          ondragover={dragOverPlaylist}
          ondragleave={() => (playlistDropActive = false)}
          ondrop={dropOnPlaylist}
        >
          <div class="panel-head">
            <div>
              <p>Drop tracks here</p>
              <h1>{activePlaylist?.playlist.name ?? 'Favorites'}</h1>
            </div>
            {#if playlistImporting}
              <RefreshCw size={22} class="spin" />
            {:else}
              <ListMusic size={24} />
            {/if}
          </div>
          <div class="track-list">
            {#each activePlaylist?.tracks ?? [] as track}
              <button class="track" onclick={() => playTrack(track)}>
                <div class="cover">{track.title.slice(0, 1).toUpperCase()}</div>
                <div class="track-meta">
                  <strong>{track.title}</strong>
                  <span>{track.artist} · {track.album}</span>
                </div>
                <span>{track.codec ?? track.format ?? 'audio'}</span>
                <time>{formatDuration(track.duration_ms)}</time>
              </button>
            {:else}
              <div class="empty">Drag tracks or audio files into this area.</div>
            {/each}
          </div>
        </section>
      </section>
    {:else if view === 'conversions'}
      <section class="panel glass">
        <div class="panel-head">
          <div>
            <p>Conversions</p>
            <h1>Local FFmpeg jobs</h1>
          </div>
          <Activity size={24} />
        </div>
        <div class="conversion-form">
          <input bind:value={convertSource} placeholder="Source audio path" />
          <input bind:value={convertOutput} placeholder="Output folder" />
          <select bind:value={convertFormat}>
            <option value="opus">Opus</option>
            <option value="mp3">MP3</option>
            <option value="flac">FLAC</option>
            <option value="aac">AAC/M4A</option>
            <option value="alac">ALAC</option>
            <option value="wav">WAV</option>
          </select>
          <button onclick={createConversion}>Convert</button>
        </div>
        <div class="jobs">
          {#each jobs as job}
            <div class="job">
              <div>
                <strong>{job.state}</strong>
                <span>{job.output_path ?? job.source_path}</span>
              </div>
              <progress max="1" value={job.progress}></progress>
              <small>{pct(job.progress)}</small>
            </div>
          {:else}
          <div class="empty">No jobs yet.</div>
          {/each}
        </div>
      </section>
    {:else}
      <section class="panel glass settings-panel">
        <div class="panel-head">
          <div>
            <p>Settings</p>
            <h1>Codec matrix</h1>
          </div>
          <Settings size={24} />
        </div>
        {#if health}
          <div class="matrix">
            {#each [health.capabilities.ffmpeg, health.capabilities.ffprobe, health.capabilities.mpv] as tool}
              <div class="tool">
                <strong>{tool.name}</strong>
                <span class:ok={tool.available}>{tool.available ? 'available' : 'missing'}</span>
                <small>{tool.version ?? tool.path ?? 'Unavailable'}</small>
              </div>
            {/each}
          </div>
          <div class="capability-columns">
            <CapabilityList title="Encoders" items={health.capabilities.encoders} />
            <CapabilityList title="Decoders" items={health.capabilities.decoders} />
            <CapabilityList title="Muxers" items={health.capabilities.muxers} />
            <CapabilityList title="Demuxers" items={health.capabilities.demuxers} />
          </div>
        {:else}
          <div class="empty">Codec matrix unavailable.</div>
        {/if}
      </section>
    {/if}
  </main>

  <footer class="player glass">
    <button class="transport" onclick={togglePlayback} aria-label={playback.playing ? 'Pause' : 'Play'}>
      {#if playback.playing}<Pause size={20} />{:else}<Play size={20} />{/if}
    </button>
    <div class="now">
      <strong>{playback.loaded_path ? playback.loaded_path.split('/').pop() : 'Nothing playing'}</strong>
      <span>{playback.playing ? 'Playing' : 'Ready'}</span>
    </div>
    <div class="volume">{Math.round(playback.volume)}%</div>
  </footer>
</div>
