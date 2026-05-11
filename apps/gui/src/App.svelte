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
    eventUrl,
    type AuraluxEvent,
    type Health,
    type JobRecord,
    type PlaybackState,
    type Track
  } from './lib/api';
  import { formatDuration, pct } from './lib/format';

  let view: 'library' | 'playlists' | 'conversions' | 'settings' = 'library';
  let health: Health | null = null;
  let tracks: Track[] = [];
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
      [health, tracks, jobs] = await Promise.all([
        apiGet<Health>('/api/health'),
        apiGet<Track[]>(`/api/library/tracks${search ? `?search=${encodeURIComponent(search)}` : ''}`),
        apiGet<JobRecord[]>('/api/jobs')
      ]);
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
      <section class="panel glass placeholder">
        <ListMusic size={34} />
        <h1>Playlists</h1>
        <p>No playlists yet.</p>
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
