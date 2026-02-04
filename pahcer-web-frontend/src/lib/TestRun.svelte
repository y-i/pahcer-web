<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type GlobalConfig } from './api';

  // Form states
  let options = $state({
    shuffle: false,
    comment: '',
    json: false,
    tag: '',
    settingFile: 'pahcer_config.toml',
    freezeBestScores: false,
    noResultFile: false,
    noCompile: false,
    extraArgs: ''
  });

  let globalConfig = $state<GlobalConfig | null>(null);

  onMount(async () => {
    try {
      const res = await api.getConfig();
      globalConfig = res.global;
      if (globalConfig?.testRunOptions) {
        options = {
          ...options,
          ...globalConfig.testRunOptions,
          comment: '', // Explicitly reset non-persisted fields
          tag: ''
        };
      }
    } catch (e) {
      console.error('Failed to load config', e);
    }
  });

  // Watch for changes to options and persist them
  $effect(() => {
    if (globalConfig) {
      const { comment, tag, ...persistableOptions } = options;
      
      // Compare to avoid infinite loop or unnecessary API calls
      const currentPersisted = globalConfig.testRunOptions;
      if (JSON.stringify(currentPersisted) !== JSON.stringify(persistableOptions)) {
        const newConfig = {
          ...globalConfig,
          testRunOptions: persistableOptions
        };
        api.saveGlobalConfig(newConfig).then(() => {
            globalConfig = newConfig;
        });
      }
    }
  });

  let logs = $state<{ type: 'stdout' | 'stderr' | 'info'; text: string }[]>([]);
  let isRunning = $state(false);
  let logContainer: HTMLDivElement;

  $effect(() => {
    if (logs.length && logContainer) {
        logContainer.scrollTop = logContainer.scrollHeight;
    }
  });

  function buildArgs(): string[] {
    const args: string[] = [];
    if (options.shuffle) args.push('--shuffle');
    if (options.comment) args.push('-c', options.comment);
    if (options.json) args.push('-j');
    if (options.tag) args.push('-t', options.tag);
    if (options.settingFile && options.settingFile !== 'pahcer_config.toml') {
      args.push('--setting-file', options.settingFile);
    }
    if (options.freezeBestScores) args.push('--freeze-best-scores');
    if (options.noResultFile) args.push('--no-result-file');
    if (options.noCompile) args.push('--no-compile');
    
    if (options.extraArgs) {
      args.push(...options.extraArgs.split(/\s+/).filter(a => a.length > 0));
    }
    return args;
  }

  async function handleRun() {
    if (isRunning) return;
    
    isRunning = true;
    logs = [{ type: 'info', text: 'Starting pahcer run...' }];
    
    const argList = buildArgs();
    logs.push({ type: 'info', text: `Command: pahcer run ${argList.join(' ')}` });
    
    try {
      await api.runPahcer(argList, (data) => {
        if (data.type === 'stdout' || data.type === 'stderr') {
          logs.push({ type: data.type, text: data.data });
        } else if (data.type === 'exit') {
          logs.push({ type: 'info', text: `Process exited with code ${data.code}` });
          isRunning = false;
        }
      });
    } catch (e) {
      logs.push({ type: 'stderr', text: `Error: ${e}` });
      isRunning = false;
    }
  }

  function clearLogs() {
    logs = [];
  }
</script>

<div class="h-full flex flex-col bg-gray-50">
  <!-- Top Control Panel -->
  <div class="bg-white border-b border-gray-200 px-6 py-4 shadow-sm flex-shrink-0 z-10">
    <div class="max-w-none w-full space-y-4">
      
      <div class="flex items-center justify-between mb-2">
        <h2 class="text-sm font-bold text-gray-500 uppercase tracking-wider">Run Configuration</h2>
        <div class="flex space-x-3">
             <button
                onclick={clearLogs}
                class="px-4 py-2 text-sm font-medium text-gray-600 bg-white border border-gray-300 rounded-lg hover:bg-gray-50 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-indigo-500 transition-colors"
            >
                Clear Logs
            </button>
            <button
                onclick={handleRun}
                disabled={isRunning}
                class="px-6 py-2 text-sm font-medium text-white bg-indigo-600 rounded-lg hover:bg-indigo-700 disabled:opacity-50 disabled:cursor-not-allowed shadow-sm focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-indigo-500 transition-all transform active:scale-95"
            >
                {#if isRunning}
                    <span class="flex items-center">
                        <svg class="animate-spin -ml-1 mr-2 h-4 w-4 text-white" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                        </svg>
                        Running...
                    </span>
                {:else}
                    Run Test
                {/if}
            </button>
        </div>
      </div>

      <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6">
        <!-- Input Group 1 -->
        <div class="space-y-4">
          <div>
            <label class="block text-xs font-semibold text-gray-700 mb-1" for="comment">Comment</label>
            <input
              id="comment"
              type="text"
              bind:value={options.comment}
              placeholder="Optional comment"
              class="block w-full px-3 py-2 bg-gray-50 border border-gray-300 rounded-md text-sm shadow-sm focus:ring-indigo-500 focus:border-indigo-500 focus:bg-white transition-colors"
            />
          </div>
          <div>
             <label class="block text-xs font-semibold text-gray-700 mb-1" for="tag">Tag</label>
             <input
              id="tag"
              type="text"
              bind:value={options.tag}
              placeholder="Optional tag"
              class="block w-full px-3 py-2 bg-gray-50 border border-gray-300 rounded-md text-sm shadow-sm focus:ring-indigo-500 focus:border-indigo-500 focus:bg-white transition-colors"
            />
          </div>
        </div>

        <!-- Input Group 2 -->
        <div class="space-y-4">
             <div>
                <label class="block text-xs font-semibold text-gray-700 mb-1" for="settingFile">Setting File</label>
                <input
                id="settingFile"
                type="text"
                bind:value={options.settingFile}
                placeholder="pahcer_config.toml"
                class="block w-full px-3 py-2 bg-gray-50 border border-gray-300 rounded-md text-sm shadow-sm focus:ring-indigo-500 focus:border-indigo-500 focus:bg-white transition-colors"
                />
            </div>
             <div>
                <label class="block text-xs font-semibold text-gray-700 mb-1" for="extra">Extra Args</label>
                <input
                    id="extra"
                    type="text"
                    bind:value={options.extraArgs}
                    placeholder="--seed 0-9"
                    class="block w-full px-3 py-2 bg-gray-50 border border-gray-300 rounded-md text-sm shadow-sm focus:ring-indigo-500 focus:border-indigo-500 focus:bg-white transition-colors"
                />
            </div>
        </div>

        <!-- Checkbox Group -->
        <div class="lg:col-span-2 grid grid-cols-2 gap-y-3 gap-x-4">
            <label class="flex items-center space-x-3 cursor-pointer group">
                <input type="checkbox" bind:checked={options.shuffle} class="h-4 w-4 text-indigo-600 border-gray-300 rounded focus:ring-indigo-500 transition-colors" />
                <span class="text-sm text-gray-600 group-hover:text-gray-900">Shuffle cases</span>
            </label>
            <label class="flex items-center space-x-3 cursor-pointer group">
                <input type="checkbox" bind:checked={options.json} class="h-4 w-4 text-indigo-600 border-gray-300 rounded focus:ring-indigo-500 transition-colors" />
                <span class="text-sm text-gray-600 group-hover:text-gray-900">JSON Output</span>
            </label>
            <label class="flex items-center space-x-3 cursor-pointer group">
                <input type="checkbox" bind:checked={options.freezeBestScores} class="h-4 w-4 text-indigo-600 border-gray-300 rounded focus:ring-indigo-500 transition-colors" />
                <span class="text-sm text-gray-600 group-hover:text-gray-900">Freeze Best Scores</span>
            </label>
             <label class="flex items-center space-x-3 cursor-pointer group">
                <input type="checkbox" bind:checked={options.noResultFile} class="h-4 w-4 text-indigo-600 border-gray-300 rounded focus:ring-indigo-500 transition-colors" />
                <span class="text-sm text-gray-600 group-hover:text-gray-900">No Result File</span>
            </label>
             <label class="flex items-center space-x-3 cursor-pointer group">
                <input type="checkbox" bind:checked={options.noCompile} class="h-4 w-4 text-indigo-600 border-gray-300 rounded focus:ring-indigo-500 transition-colors" />
                <span class="text-sm text-gray-600 group-hover:text-gray-900">No Compile</span>
            </label>
        </div>
      </div>
    </div>
  </div>

  <!-- Console Output -->
  <div class="flex-1 flex flex-col min-h-0 bg-gray-950 text-gray-300 font-mono text-sm relative">
    <div class="absolute top-0 inset-x-0 h-px bg-gray-800"></div>
    <div class="px-6 py-2 bg-gray-900/50 border-b border-gray-800 flex justify-between items-center select-none backdrop-blur-sm sticky top-0">
      <div class="flex items-center space-x-2">
        <svg class="w-4 h-4 text-gray-500" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z"></path></svg>
        <span class="font-medium text-gray-400 text-xs uppercase tracking-wider">Terminal Output</span>
      </div>
      {#if isRunning}
        <div class="flex items-center space-x-2 px-2 py-1 bg-green-900/20 rounded border border-green-900/30">
            <span class="relative flex h-2 w-2">
              <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-green-400 opacity-75"></span>
              <span class="relative inline-flex rounded-full h-2 w-2 bg-green-500"></span>
            </span>
            <span class="text-xs font-medium text-green-400">Processing</span>
        </div>
      {/if}
    </div>
    <div 
        bind:this={logContainer}
        class="flex-1 overflow-y-auto p-6 space-y-1 scroll-smooth"
    >
      {#each logs as log}
        <div class="leading-relaxed break-words {log.type === 'stderr' ? 'text-red-400' : log.type === 'info' ? 'text-blue-400 font-bold' : 'text-gray-300'}">
          {log.text}
        </div>
      {/each}
      {#if logs.length === 0}
        <div class="h-full flex items-center justify-center text-gray-700 select-none">
            <div class="text-center">
                 <svg class="w-12 h-12 mx-auto mb-3 opacity-20" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M14.752 11.168l-3.197-2.132A1 1 0 0010 9.87v4.263a1 1 0 001.555.832l3.197-2.132a1 1 0 000-1.664z"></path><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 12a9 9 0 11-18 0 9 9 0 0118 0z"></path></svg>
                 <p>Ready to run tests</p>
            </div>
        </div>
      {/if}
    </div>
  </div>
</div>