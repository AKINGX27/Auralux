export function formatDuration(ms?: number | null): string {
  if (!ms) return '--:--';
  const seconds = Math.floor(ms / 1000);
  const minutes = Math.floor(seconds / 60);
  const rest = seconds % 60;
  return `${minutes}:${rest.toString().padStart(2, '0')}`;
}

export function pct(value: number): string {
  return `${Math.round(value * 100)}%`;
}

