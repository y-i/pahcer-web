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
    iframeSrc = `${visualizerUrl}?seed=${seed}&scale=${scale}`;
  }

  $effect(() => {
    if (selectedRow) {
        updateVisualizer();
    }
  });
</script>

<div class="h-full flex flex-col bg-gray-50 overflow-hidden">
  {#if isLoading}
    <div class="flex-1 flex items-center justify-center text-gray-500 space-x-2">
        <svg class="animate-spin h-5 w-5 text-indigo-600" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
        <span>Loading history...</span>
    </div>
  {:else if error}
    <div class="flex-1 flex items-center justify-center text-red-500">{error}</div>
  {:else}
    <div class="flex-1 flex overflow-hidden">
      <!-- List View -->
      <div class="{selectedRow && config.visualizerPosition === 'right' ? 'w-1/2 border-r' : selectedRow && config.visualizerPosition === 'left' ? 'w-1/2 border-l order-2' : 'w-full'} flex flex-col bg-white transition-all duration-300 shadow-sm z-0">
        <div class="px-6 py-4 border-b border-gray-200 bg-white flex justify-between items-center flex-shrink-0 z-10">
            <h2 class="text-lg font-bold text-gray-900 tracking-tight">Execution History</h2>
            <button 
                onclick={() => api.getList().then(res => listData = res.parsed)} 
                class="inline-flex items-center px-3 py-1.5 border border-transparent text-xs font-medium rounded-md text-indigo-700 bg-indigo-100 hover:bg-indigo-200 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-indigo-500 transition-colors"
            >
                Refresh
            </button>
        </div>
        <div class="flex-1 overflow-auto">
            <table class="min-w-full divide-y divide-gray-200">
                <thead class="bg-gray-50 sticky top-0 z-10 shadow-sm">
                    <tr>
                        {#if listData.length > 0}
                            {#each Object.keys(listData[0]) as key}
                                <th scope="col" class="px-6 py-3 text-left text-xs font-semibold text-gray-500 uppercase tracking-wider whitespace-nowrap bg-gray-50">
                                    {key}
                                </th>
                            {/each}
                        {:else}
                            <th class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Status</th>
                        {/if}
                    </tr>
                </thead>
                <tbody class="bg-white divide-y divide-gray-200">
                    {#each listData as row, i}
                        <tr 
                            class="group hover:bg-indigo-50/50 cursor-pointer transition-colors duration-150 ease-in-out {selectedRow === row ? 'bg-indigo-50' : ''}"
                            onclick={() => selectRow(row)}
                        >
                            {#each Object.values(row) as val}
                                <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-700 group-hover:text-gray-900 font-mono">{val}</td>
                            {/each}
                        </tr>
                    {/each}
                </tbody>
            </table>
            {#if listData.length === 0}
                <div class="flex flex-col items-center justify-center p-12 text-center">
                    <div class="rounded-full bg-gray-100 p-3 mb-4">
                        <svg class="h-6 w-6 text-gray-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2" />
                        </svg>
                    </div>
                    <h3 class="text-sm font-medium text-gray-900">No history found</h3>
                    <p class="mt-1 text-sm text-gray-500">Run <code>pahcer list</code> or use the Test Run tab to generate results.</p>
                    {#if rawOutput}
                        <div class="mt-6 w-full max-w-lg">
                            <p class="text-xs font-semibold text-gray-500 mb-2 uppercase tracking-wide">Raw Output</p>
                            <pre class="text-xs text-left bg-gray-50 p-4 rounded-lg border border-gray-200 overflow-x-auto text-gray-600">{rawOutput}</pre>
                        </div>
                    {/if}
                </div>
            {/if}
        </div>
      </div>

      <!-- Visualizer View -->
      {#if selectedRow}
        <div class="{config.visualizerPosition === 'left' ? 'order-1' : ''} w-1/2 flex flex-col bg-white border-l border-gray-200 shadow-xl z-20">
            <div class="px-4 py-3 border-b border-gray-200 bg-white flex items-center space-x-6 shadow-sm z-10">
                <span class="text-xs font-bold text-gray-400 uppercase tracking-wider">Visualizer Controls</span>
                <div class="h-4 w-px bg-gray-300"></div>
                <div class="flex items-center space-x-3">
                    <label class="text-sm font-medium text-gray-600">Seed</label>
                    <input 
                        type="number" 
                        bind:value={seed} 
                        class="w-24 px-2 py-1 bg-gray-50 border border-gray-300 rounded text-sm focus:ring-1 focus:ring-indigo-500 focus:border-indigo-500" 
                    />
                </div>
                <div class="flex items-center space-x-3">
                    <label class="text-sm font-medium text-gray-600">Scale</label>
                    <input 
                        type="number" 
                        step="0.1" 
                        bind:value={scale} 
                        class="w-20 px-2 py-1 bg-gray-50 border border-gray-300 rounded text-sm focus:ring-1 focus:ring-indigo-500 focus:border-indigo-500" 
                    />
                </div>
                <div class="flex-1"></div>
                <button 
                    onclick={() => selectedRow = null} 
                    class="p-1 rounded-md text-gray-400 hover:text-gray-600 hover:bg-gray-100 transition-colors"
                    title="Close Visualizer"
                >
                    <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"></path></svg>
                </button>
            </div>
            <div class="flex-1 relative bg-gray-50">
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