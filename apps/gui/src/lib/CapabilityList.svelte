<script lang="ts">
  export let title: string;
  export let items: string[];
</script>

<div class="capability-list">
  <strong>{title}</strong>
  <div>
    {#each items.slice(0, 36) as item}
      <span>{item}</span>
    {:else}
      <em>Unavailable</em>
    {/each}
  </div>
</div>

