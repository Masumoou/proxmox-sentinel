<script lang="ts">
  import { onMount, tick } from 'svelte';
  import uPlot from 'uplot';
  import type { Options, AlignedData } from 'uplot';
  import 'uplot/dist/uPlot.min.css';

  let { nodeName } = $props();
  let chartEl = $state<HTMLDivElement>();
  let chart: uPlot | null = null;

  async function fetchHistory() {
    try {
      const res = await fetch(`/api/v1/history/node/${nodeName}/metrics`);
      if (!res.ok) throw new Error('Failed to fetch history');
      const data = await res.json();
      
      // Transform data: [[timestamps], [cpu], [mem]]
      // Timestamps must be Unix seconds
      const timestamps: number[] = [];
      const cpu: number[] = [];
      const mem: number[] = [];

      data.forEach((row: any) => {
        timestamps.push(new Date(row.ts).getTime() / 1000);
        cpu.push(row.cpu_usage * 100);
        mem.push(row.mem_total > 0 ? (row.mem_used / row.mem_total) * 100 : 0);
      });

      return [timestamps, cpu, mem] as AlignedData;
    } catch (e) {
      console.error('History fetch error', e);
      return null;
    }
  }

  async function initChart() {
    const data = await fetchHistory();
    if (!data || !chartEl) return;

    await tick();

    const opts: Options = {
      width: chartEl!.offsetWidth || 300,
      height: 60,
      scales: {
        x: { time: true },
        y: { range: [0, 100] }
      },
      axes: [
        { show: false },
        { show: false }
      ],
      series: [
        {},
        {
          stroke: "#22d3ee",
          width: 2,
          points: { show: false }
        },
        {
          stroke: "#f472b6",
          width: 2,
          points: { show: false }
        }
      ]
    };

    if (chartEl) {
      chart = new uPlot(opts, data, chartEl);
    }
  }

  onMount(() => {
    initChart();
    
    // Refresh every 5 minutes
    const interval = setInterval(async () => {
      const newData = await fetchHistory();
      if (newData && chart) {
        chart.setData(newData);
      }
    }, 300000);

    return () => {
      clearInterval(interval);
      chart?.destroy();
    };
  });
</script>

<div bind:this={chartEl} class="w-full h-[60px] opacity-80 hover:opacity-100 transition-opacity"></div>

<style>
  .w-full { width: 100%; }
  .h-\[60px\] { height: 60px; }
  .opacity-80 { opacity: 0.8; }
  .hover\:opacity-100:hover { opacity: 1; }
  .transition-opacity { transition: opacity 0.3s ease; }
</style>
