<script lang="ts">
  import type { GraphRow } from '$lib/util/graph';

  /** Celda del grafo de ramas de un commit (SVG de la altura de la fila). */
  let { row, columns, height, merge = false }: { row: GraphRow; columns: number; height: number; merge?: boolean } = $props();

  const LANE_W = 14;
  const PALETTE = ['--accent', '--success', '--warning', '--info', '--danger', '--tok-keyword'];
  const x = (lane: number) => 9 + lane * LANE_W;
  const color = (lane: number) => `var(${PALETTE[lane % PALETTE.length]})`;
  const width = $derived(Math.min(columns, 8) * LANE_W + 6);
  const mid = $derived(height / 2);
  // Con más de 8 carriles las columnas extra se aplastan en la última.
  const cl = (lane: number) => Math.min(lane, 7);
</script>

<svg class="graph" {width} {height} viewBox="0 0 {width} {height}" aria-hidden="true" focusable="false">
  {#each row.through as k (k)}
    <line x1={x(cl(k))} y1="0" x2={x(cl(k))} y2={height} stroke={color(k)} />
  {/each}
  {#if row.incoming}
    <line x1={x(cl(row.lane))} y1="0" x2={x(cl(row.lane))} y2={mid} stroke={color(row.lane)} />
  {/if}
  {#each row.joins as j (j)}
    <path d="M {x(cl(j))} 0 C {x(cl(j))} {height * 0.4}, {x(cl(row.lane))} {height * 0.1}, {x(cl(row.lane))} {mid}" stroke={color(j)} fill="none" />
  {/each}
  {#each row.parents as p, i (i + ':' + p)}
    {#if p === row.lane}
      <line x1={x(cl(row.lane))} y1={mid} x2={x(cl(p))} y2={height} stroke={color(p)} />
    {:else}
      <path d="M {x(cl(row.lane))} {mid} C {x(cl(row.lane))} {height * 0.9}, {x(cl(p))} {height * 0.6}, {x(cl(p))} {height}" stroke={color(p)} fill="none" />
    {/if}
  {/each}
  <circle cx={x(cl(row.lane))} cy={mid} r="4" stroke={color(row.lane)} fill={merge ? color(row.lane) : 'var(--bg-surface)'} />
</svg>

<style>
  .graph {
    flex: none;
    align-self: stretch;
    overflow: visible;
  }
  line,
  path {
    stroke-width: 2;
    stroke-linecap: round;
  }
  circle {
    stroke-width: 2;
  }
</style>
