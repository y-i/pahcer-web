<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type GlobalConfig } from './api';

  let listData = $state<Record<string, string>[]>([]);
  let rawOutput = $state('');
  let isLoading = $state(true);
  let error = $state('');
  let selectedRow = $state<Record<string, string> | null>(null);
  
  let visualizerUrl = $state('/visualizer.html');
  let config = $state<GlobalConfig>({
    visualizerPosition: 'right',
    visualizerUrl: '',
    defaultSeed: 0,
    defaultScale: 1.0
  });

  // Visualizer controls
  let seed = $state(0);
  let scale = $state(1.0);
  let iframeSrc = $state('');

  onMount(async () => {
    try {
      const [listRes, configRes] = await Promise.all([
        api.getList(),
        api.getConfig()
      ]);
      listData = listRes.parsed;
      rawOutput = listRes.raw;
      config = configRes.global;
      
      seed = config.defaultSeed;
      scale = config.defaultScale;
    } catch (e) {
      error = 'Failed to load data';
      console.error(e);
    } finally {
      isLoading = false;
    }
  });

  function selectRow(row: Record<string, string>) {
    selectedRow = row;
    updateVisualizer();
  }

  function updateVisualizer() {
    // Assuming visualizer accepts query params like ?seed=X&input=...
    // Or we just update the seed for now.
    // If we have selectedRow, we might want to use its specific seed if available in columns
    // For now, use the manual controls
    iframeSrc = `${visualizerUrl}?seed=${seed}&scale=${scale}`;
  }

  $effect(() => {
    // React to seed/scale changes
    if (selectedRow) {
        updateVisualizer();
    }
  });
</script>

<div class="h-full flex flex-col overflow-hidden bg-white">
  {#if isLoading}
    <div class="p-8 text-center text-gray-500">Loading history...</div>
  {:else if error}
    <div class="p-8 text-center text-red-500">{error}</div>
  {:else}
    <div class="flex-1 flex overflow-hidden">
      <!-- List View -->
      <div class="{selectedRow && config.visualizerPosition === 'right' ? 'w-1/2 border-r' : selectedRow && config.visualizerPosition === 'left' ? 'w-1/2 border-l order-2' : 'w-full'} flex flex-col transition-all duration-300">
        <div class="p-4 border-b bg-gray-50 flex justify-between items-center">
            <h2 class="font-semibold text-gray-700">Execution History</h2>
            <button onclick={() => api.getList().then(res => listData = res.parsed)} class="text-sm text-indigo-600 hover:text-indigo-800">Refresh</button>
        </div>
        <div class="flex-1 overflow-auto">
            <table class="min-w-full divide-y divide-gray-200">
                <thead class="bg-gray-50">
                    <tr>
                        {#if listData.length > 0}
                            {#each Object.keys(listData[0]) as key}
                                <th scope="col" class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider sticky top-0 bg-gray-50">
                                    {key}
                                </th>
                            {/each}
                        {:else}
                            <th class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">No Data</th>
                        {/if}
                    </tr>
                </thead>
                <tbody class="bg-white divide-y divide-gray-200">
                    {#each listData as row, i}
                        <button 
                            class="table-row hover:bg-indigo-50 cursor-pointer w-full text-left {selectedRow === row ? 'bg-indigo-50' : ''}"
                            onclick={() => selectRow(row)}
                        >
                            {#each Object.values(row) as val}
                                <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-900">{val}</td>
                            {/each}
                        </button>
                    {/each}
                </tbody>
            </table>
            {#if listData.length === 0}
                <div class="p-8 text-center text-gray-400">
                    No history found. Run <code>pahcer list</code> or use the Test Run tab.
                    {#if rawOutput}
                        <pre class="mt-4 text-xs text-left bg-gray-100 p-2 rounded">{rawOutput}</pre>
                    {/if}
                </div>
            {/if}
        </div>
      </div>

      <!-- Visualizer View -->
      {#if selectedRow}
        <div class="{config.visualizerPosition === 'left' ? 'order-1' : ''} w-1/2 flex flex-col bg-gray-100 border-gray-200">
            <div class="p-2 border-b bg-white flex items-center space-x-4 shadow-sm z-10">
                <div class="flex items-center space-x-2">
                    <label class="text-xs font-medium text-gray-500">Seed</label>
                    <input type="number" bind:value={seed} class="w-24 px-2 py-1 border rounded text-sm" />
                </div>
                <div class="flex items-center space-x-2">
                    <label class="text-xs font-medium text-gray-500">Scale</label>
                    <input type="number" step="0.1" bind:value={scale} class="w-20 px-2 py-1 border rounded text-sm" />
                </div>
                <div class="flex-1"></div>
                <button onclick={() => selectedRow = null} class="text-gray-400 hover:text-gray-600">
                    <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"></path></svg>
                </button>
            </div>
            <div class="flex-1 relative bg-white">
                <iframe 
                    title="Visualizer"
                    src={iframeSrc} 
                    class="absolute inset-0 w-full h-full border-0"
                ></iframe>
            </div>
        </div>
      {/if}
    </div>
  {/if}
</div>