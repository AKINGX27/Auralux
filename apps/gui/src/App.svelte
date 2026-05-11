<script lang="ts">
  import {
    Activity,
    Album,
    FileAudio,
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

  type View = 'library' | 'playlists' | 'conversions' | 'settings';

  const viewCopy: Record<
    View,
    {
      label: string;
      title: string;
      subtitle: string;
    }
  > = {
    library: {
      label: 'Library',
      title: 'Music, cleanly laid out',
      subtitle: 'Scan folders, browse tracks, and drag music into playlists.'
    },
    playlists: {
      label: 'Playlists',
      title: 'Keep the sets that matter',
      subtitle: 'Drop audio from Explorer or drag from the library to build a playlist.'
    },
    conversions: {
      label: 'Conversions',
      title: 'Transcode on the local machine',
      subtitle: 'Queue FFmpeg jobs without leaving the app.'
    },
    settings: {
      label: 'Settings',
      title: 'Codec capability matrix',
      subtitle: 'See what the local ffmpeg, ffprobe, and mpv build can actually do.'
    }
  };

  const audioExtensions = new Set([
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
  ]);

  let view: View = 'library';
  let health: Health | null = null;
  let tracks: Track[] = [];
  let playlists: Playlist[] = [];
  let activePlaylist: PlaylistDetail | null = null;
  let selectedTrackId: number | null = null;
  let selectedPlaylistId: number | null = null;
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
  let musicFileInput: HTMLInputElement | null = null;
  let refreshRetryCount = 0;

  const hasTauriRuntime = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

  $: activePlaylistTracks = activePlaylist?.tracks ?? [];
  $: selectedTrack =
    tracks.find((track) => track.id === selectedTrackId) ??
    activePlaylistTracks.find((track) => track.id === selectedTrackId) ??
    activePlaylistTracks[0] ??
    tracks[0] ??
    null;
  $: nowPlayingTitle = playback.loaded_path ? basename(playback.loaded_path) : 'Nothing playing';
  $: selectedFormats = collectFormats(view === 'playlists' ? activePlaylistTracks : tracks);

  const nav = [
    { id: 'library', label: 'Library', icon: Library },
    { id: 'playlists', label: 'Playlists', icon: ListMusic },
    { id: 'conversions', label: 'Convert', icon: SlidersHorizontal },
    { id: 'settings', label: 'Settings', icon: Settings }
  ] as const;

  onMount(() => {
    void refresh();
    connectEvents();
    void connectTauriOpenFiles();
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
      refreshRetryCount = 0;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
      status = 'Local core unavailable';
      if (hasTauriRuntime && refreshRetryCount < 8) {
        refreshRetryCount += 1;
        window.setTimeout(() => void refresh(), 750);
      }
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
    const current =
      (selectedPlaylistId && playlists.find((playlist) => playlist.id === selectedPlaylistId)) ||
      (activePlaylist && playlists.find((playlist) => playlist.id === activePlaylist?.playlist.id));
    const selected = current ?? playlists[0];
    selectedPlaylistId = selected.id;
    activePlaylist = await apiGet<PlaylistDetail>(`/api/playlists/${selected.id}`);
    if (!selectedTrackId || !activePlaylist.tracks.some((track) => track.id === selectedTrackId)) {
      selectedTrackId = activePlaylist.tracks[0]?.id ?? tracks[0]?.id ?? null;
    }
  }

  async function refreshPlaylists(preferredId?: number) {
    try {
      playlists = await apiGet<Playlist[]>('/api/playlists');
      if (preferredId) {
        selectedPlaylistId = preferredId;
        activePlaylist = await apiGet<PlaylistDetail>(`/api/playlists/${preferredId}`);
        if (!selectedTrackId || !activePlaylist.tracks.some((track) => track.id === selectedTrackId)) {
          selectedTrackId = activePlaylist.tracks[0]?.id ?? selectedTrackId;
        }
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
        selectedPlaylistId = playlist.id;
        selectedTrackId = activePlaylist.tracks.at(-1)?.id ?? selectedTrackId;
        tracks = await apiGet<Track[]>(`/api/library/tracks${search ? `?search=${encodeURIComponent(search)}` : ''}`);
      } else if (Number.isFinite(trackId) && trackId > 0) {
        activePlaylist = await apiPost<PlaylistDetail>(`/api/playlists/${playlist.id}/tracks`, { track_id: trackId });
        selectedPlaylistId = playlist.id;
        selectedTrackId = trackId;
      } else {
        return;
      }
      playlists = await apiGet<Playlist[]>('/api/playlists');
      await syncActivePlaylist();
      view = 'playlists';
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      playlistImporting = false;
    }
  }

  async function chooseMusicFiles(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    await importFilesToPlaylist(Array.from(input.files ?? []));
    input.value = '';
  }

  function openMusicPicker() {
    musicFileInput?.click();
  }

  async function connectTauriOpenFiles() {
    if (!hasTauriRuntime) return;
    try {
      const [{ listen }, { invoke }] = await Promise.all([import('@tauri-apps/api/event'), import('@tauri-apps/api/core')]);
      const takeOpenFiles = () => invoke<string[]>('take_open_files');
      await listen('auralux:open-files', () => {
        void takeOpenFiles().then(importPathsToPlaylist);
      });
      await importPathsToPlaylist(await takeOpenFiles());
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  async function importPathsToPlaylist(paths: string[]) {
    const audioPaths = Array.from(new Set(paths.filter(isAudioPath)));
    if (audioPaths.length === 0) return;
    playlistImporting = true;
    playlistDropActive = false;
    view = 'playlists';
    try {
      const playlist = activePlaylist?.playlist ?? (await apiPost<Playlist>('/api/playlists', { name: 'Favorites' }));
      selectedPlaylistId = playlist.id;
      activePlaylist = await apiPost<PlaylistDetail>(`/api/playlists/${playlist.id}/import-paths`, { paths: audioPaths });
      selectedTrackId = activePlaylist.tracks.at(-1)?.id ?? selectedTrackId;
      playlists = await apiGet<Playlist[]>('/api/playlists');
      tracks = await apiGet<Track[]>(`/api/library/tracks${search ? `?search=${encodeURIComponent(search)}` : ''}`);
      status = `${audioPaths.length} file${audioPaths.length === 1 ? '' : 's'} added`;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      playlistImporting = false;
    }
  }

  async function importFilesToPlaylist(files: File[]) {
    const audioFiles = files.filter(isAudioFile);
    if (audioFiles.length === 0) return;
    playlistImporting = true;
    playlistDropActive = false;
    view = 'playlists';
    try {
      const playlist = activePlaylist?.playlist ?? (await apiPost<Playlist>('/api/playlists', { name: 'Favorites' }));
      const form = new FormData();
      for (const file of audioFiles) form.append('files', file, file.name);
      activePlaylist = await apiUpload<PlaylistDetail>(`/api/playlists/${playlist.id}/import`, form);
      selectedPlaylistId = playlist.id;
      selectedTrackId = activePlaylist.tracks.at(-1)?.id ?? selectedTrackId;
      playlists = await apiGet<Playlist[]>('/api/playlists');
      tracks = await apiGet<Track[]>(`/api/library/tracks${search ? `?search=${encodeURIComponent(search)}` : ''}`);
      status = `${audioFiles.length} file${audioFiles.length === 1 ? '' : 's'} added`;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      playlistImporting = false;
    }
  }

  function isAudioFile(file: File) {
    if (file.type.startsWith('audio/')) return true;
    const extension = file.name.split('.').pop()?.toLowerCase() ?? '';
    return audioExtensions.has(extension);
  }

  function isAudioPath(path: string) {
    const extension = path.split(/[\\/]/).pop()?.split('.').pop()?.toLowerCase() ?? '';
    return audioExtensions.has(extension);
  }

  async function ensureDefaultPlaylist() {
    try {
      const playlist = await apiPost<Playlist>('/api/playlists', { name: 'Favorites' });
      playlists = await apiGet<Playlist[]>('/api/playlists');
      selectedPlaylistId = playlist.id;
      activePlaylist = await apiGet<PlaylistDetail>(`/api/playlists/${playlist.id}`);
      selectedTrackId = activePlaylist.tracks[0]?.id ?? null;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  async function selectPlaylist(playlist: Playlist) {
    try {
      selectedPlaylistId = playlist.id;
      activePlaylist = await apiGet<PlaylistDetail>(`/api/playlists/${playlist.id}`);
      selectedTrackId = activePlaylist.tracks[0]?.id ?? selectedTrackId;
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
      selectedTrackId = track.id;
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

  function basename(path: string) {
    return path.split(/[\\/]/).pop() ?? path;
  }

  function collectFormats(items: Track[]) {
    return Array.from(
      new Set(
        items
          .map((track) => track.codec ?? track.format ?? 'audio')
          .filter((value) => value && value.trim())
      )
    ).slice(0, 5);
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

    <nav class="nav-grid">
      {#each nav as item}
        <button class:active={view === item.id} onclick={() => (view = item.id)} aria-label={item.label}>
          <svelte:component this={item.icon} size={18} />
          <span>{item.label}</span>
        </button>
      {/each}
    </nav>

    <div class="sidebar-meta">
      <div>
        <strong>{tracks.length}</strong>
        <span>Tracks</span>
      </div>
      <div>
        <strong>{playlists.length}</strong>
        <span>Playlists</span>
      </div>
      <div>
        <strong>{jobs.length}</strong>
        <span>Jobs</span>
      </div>
    </div>
  </aside>

  <main class="main">
    <header class="hero glass">
      <div class="hero-copy">
        <p>{viewCopy[view].label}</p>
        <h1>{viewCopy[view].title}</h1>
        <span>{viewCopy[view].subtitle}</span>
      </div>
      <div class="hero-actions">
        <div class="search shell-search">
          <Search size={17} />
          <input bind:value={search} onkeydown={(event) => event.key === 'Enter' && refresh()} placeholder="Search library" />
        </div>
        <button class="icon-button" onclick={refresh} aria-label="Refresh" title="Refresh">
          <RefreshCw size={18} class={busy ? 'spin' : ''} />
        </button>
      </div>
    </header>

    {#if error}
      <section class="banner glass">{error}</section>
    {/if}

    <input
      class="visually-hidden"
      bind:this={musicFileInput}
      type="file"
      accept="audio/*,.aac,.aif,.aiff,.alac,.ape,.caf,.dff,.dsf,.flac,.m4a,.mka,.mp2,.mp3,.mp4,.mpc,.oga,.ogg,.opus,.tak,.tta,.wav,.weba,.wma,.wv"
      multiple
      onchange={chooseMusicFiles}
    />

    <section class="content-grid" class:playlist-mode={view === 'playlists'}>
      <section class="surface glass">
        {#if view === 'library'}
          <div class="surface-head">
            <div>
              <p>Library</p>
              <h2>Browse and queue</h2>
            </div>
            <div class="surface-actions">
              <button class="icon-button" onclick={openMusicPicker} aria-label="Add music files" title="Add music files">
                <FileAudio size={18} />
              </button>
              <button class="icon-button" onclick={runScan} aria-label="Scan folder" title="Scan folder">
                <FolderPlus size={18} />
              </button>
            </div>
          </div>

          <div class="command-row library-command-row">
            <input bind:value={scanPath} placeholder="/Users/name/Music or /sdcard/Music" />
            <button onclick={openMusicPicker}><FileAudio size={17} />Add files</button>
            <button onclick={runScan}><FolderPlus size={17} />Scan</button>
          </div>

          <div class="track-list">
            {#each tracks as track}
              <button
                class:selected={selectedTrackId === track.id}
                class="track"
                draggable="true"
                ondragstart={(event) => dragTrack(event, track)}
                onclick={() => playTrack(track)}
              >
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
        {:else if view === 'playlists'}
          <div class="playlist-shell">
            <aside class="playlist-rail">
              <div class="surface-head tight">
                <div>
                  <p>Playlists</p>
                  <h2>{playlists.length} lists</h2>
                </div>
                <button class="icon-button" onclick={ensureDefaultPlaylist} aria-label="New playlist" title="New playlist">
                  <ListMusic size={18} />
                </button>
              </div>
              <div class="playlist-list">
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
              </div>
            </aside>

            <section class="playlist-detail">
              <div class="surface-head">
                <div>
                  <p>Playlist editor</p>
                  <h2>{activePlaylist?.playlist.name ?? 'Favorites'}</h2>
                </div>
                {#if playlistImporting}
                  <RefreshCw size={20} class="spin" />
                {:else}
                  <ListMusic size={20} />
                {/if}
              </div>

              <div class="playlist-actions">
                <button class="primary-action" onclick={openMusicPicker}>
                  <FileAudio size={17} />
                  Add music files
                </button>
              </div>

              <div
                class="dropzone"
                class:drop-active={playlistDropActive}
                role="region"
                ondragover={dragOverPlaylist}
                ondragleave={() => (playlistDropActive = false)}
                ondrop={dropOnPlaylist}
                aria-label="Playlist drop area"
              >
                <div>
                  <strong>{playlistImporting ? 'Importing files' : 'Add tracks here'}</strong>
                  <span>{playlistImporting ? 'Uploading and indexing on the local daemon.' : 'Choose music files on Android, or drag tracks/files on desktop.'}</span>
                </div>
              </div>

              <div class="playlist-table-head">
                <span>#</span>
                <span>Title</span>
                <span>Codec</span>
                <span>Length</span>
              </div>

              <div class="track-list playlist-track-list">
                {#each activePlaylistTracks as track, index}
                  <button class:selected={selectedTrackId === track.id} class="track playlist-track" onclick={() => playTrack(track)}>
                    <span class="track-index">{index + 1}</span>
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
          </div>
        {:else if view === 'conversions'}
          <div class="surface-head">
            <div>
              <p>Conversions</p>
              <h2>Local FFmpeg jobs</h2>
            </div>
            <Activity size={22} />
          </div>

          <div class="conversion-card">
            <div class="command-row conversion-grid">
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
        {:else}
          <div class="surface-head">
            <div>
              <p>Settings</p>
              <h2>Codec capability matrix</h2>
            </div>
            <Settings size={22} />
          </div>

          {#if health}
            <div class="tool-rail">
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
        {/if}
      </section>

      {#if view !== 'playlists'}
      <aside class="context-stack">
        <section class="glass context-panel">
          <div class="surface-head">
            <div>
              <p>Now playing</p>
              <h2>{nowPlayingTitle}</h2>
            </div>
            <button class="transport" onclick={togglePlayback} aria-label={playback.playing ? 'Pause' : 'Play'}>
              {#if playback.playing}<Pause size={18} />{:else}<Play size={18} />{/if}
            </button>
          </div>
          <div class="context-stat">
            <strong>{playback.playing ? 'Playing' : 'Ready'}</strong>
            <span>{Math.round(playback.volume)}% volume</span>
          </div>
          <div class="context-stat">
            <strong>{selectedTrack?.title ?? 'Nothing selected'}</strong>
            <span>{selectedTrack ? `${selectedTrack.artist} · ${selectedTrack.album}` : 'Choose a track to preview details.'}</span>
          </div>
          <div class="context-chip-row">
            {#each selectedFormats as format}
              <span>{format}</span>
            {/each}
          </div>
        </section>

        <section class="glass context-panel">
          <div class="surface-head">
            <div>
              <p>Playback queue</p>
              <h2>Up next</h2>
            </div>
            <Shuffle size={18} />
          </div>
          <div class="queue">
            {#each tracks.slice(0, 8) as track}
              <button class:selected={selectedTrackId === track.id} class="queue-item" onclick={() => playTrack(track)}>
                <span>{track.title}</span>
                <small>{track.artist}</small>
              </button>
            {:else}
              <div class="empty compact">No queued tracks.</div>
            {/each}
          </div>
        </section>
      </aside>
      {/if}
    </section>
  </main>

  <footer class="player glass">
    <button class="transport" onclick={togglePlayback} aria-label={playback.playing ? 'Pause' : 'Play'}>
      {#if playback.playing}<Pause size={20} />{:else}<Play size={20} />{/if}
    </button>
    <div class="now">
      <strong>{nowPlayingTitle}</strong>
      <span>{playback.playing ? 'Playing now' : 'Ready to play'}</span>
    </div>
    <div class="player-meta">
      <span>{status}</span>
      <strong>{playlists.length} playlists</strong>
    </div>
  </footer>
</div>
