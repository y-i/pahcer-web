<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type GlobalConfig } from './api';

  // Form states
  let options = $state({
    shuffle: false,
    comment: '',
    tag: '',
    settingFile: 'pahcer_config.toml',
    freezeBestScores: false,
    noCompile: false
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
    if (options.tag) args.push('-t', options.tag);
    if (options.settingFile && options.settingFile !== 'pahcer_config.toml') {
      args.push('--setting-file', options.settingFile);
    }
    if (options.freezeBestScores) args.push('--freeze-best-scores');
    if (options.noCompile) args.push('--no-compile');
    
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

<div class="h-full flex flex-col bg-gray-50 p-6 gap-6 overflow-hidden">

  <!-- Top Control Panel Card -->

  <div class="bg-white rounded-xl shadow-sm border border-gray-200 px-6 py-5 flex-shrink-0 z-10">

    <div class="max-w-none w-full space-y-6">

      

      <div class="flex items-center justify-between">

        <h2 class="text-sm font-bold text-gray-400 uppercase tracking-widest">Run Configuration</h2>

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

            <label class="block text-xs font-semibold text-gray-600 mb-1" for="comment">Comment</label>

            <input

              id="comment"

              type="text"

              bind:value={options.comment}

              placeholder="Optional comment"

              class="block w-full px-3 py-2 bg-gray-50 border border-gray-300 rounded-lg text-sm shadow-sm focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500 focus:bg-white transition-all outline-none"

            />

          </div>

          <div>

             <label class="block text-xs font-semibold text-gray-600 mb-1" for="tag">Tag</label>

             <input

              id="tag"

              type="text"

              bind:value={options.tag}

              placeholder="Optional tag"

              class="block w-full px-3 py-2 bg-gray-50 border border-gray-300 rounded-lg text-sm shadow-sm focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500 focus:bg-white transition-all outline-none"

            />

          </div>

        </div>



        <!-- Input Group 2 -->

        <div class="space-y-4">

             <div>

                <label class="block text-xs font-semibold text-gray-600 mb-1" for="settingFile">Setting File</label>

                <input

                id="settingFile"

                type="text"

                bind:value={options.settingFile}

                placeholder="pahcer_config.toml"

                class="block w-full px-3 py-2 bg-gray-50 border border-gray-300 rounded-lg text-sm shadow-sm focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500 focus:bg-white transition-all outline-none"

                />

            </div>

        </div>



        <!-- Checkbox Group -->

        <div class="lg:col-span-2 grid grid-cols-2 gap-y-3 gap-x-4">

            <label class="flex items-center space-x-3 cursor-pointer group">

                <input type="checkbox" bind:checked={options.shuffle} class="h-4 w-4 text-indigo-600 border-gray-300 rounded focus:ring-indigo-500 transition-colors" />

                <span class="text-sm text-gray-600 group-hover:text-gray-900 font-medium">Shuffle cases</span>

            </label>

            <label class="flex items-center space-x-3 cursor-pointer group">

                <input type="checkbox" bind:checked={options.freezeBestScores} class="h-4 w-4 text-indigo-600 border-gray-300 rounded focus:ring-indigo-500 transition-colors" />

                <span class="text-sm text-gray-600 group-hover:text-gray-900 font-medium">Freeze Best Scores</span>

            </label>

             <label class="flex items-center space-x-3 cursor-pointer group">

                <input type="checkbox" bind:checked={options.noCompile} class="h-4 w-4 text-indigo-600 border-gray-300 rounded focus:ring-indigo-500 transition-colors" />

                <span class="text-sm text-gray-600 group-hover:text-gray-900 font-medium">No Compile</span>

            </label>

        </div>

      </div>

    </div>

  </div>



  <!-- Console Output Card -->

  <div class="flex-1 flex flex-col min-h-0 bg-gray-950 text-gray-300 font-mono text-sm relative rounded-xl shadow-2xl overflow-hidden border border-gray-800">

    <div class="px-6 py-2 bg-gray-900/80 border-b border-gray-800 flex justify-between items-center select-none backdrop-blur-sm sticky top-0 z-10">

      <div class="flex items-center space-x-2">

        <svg class="w-4 h-4 text-gray-500" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z"></path></svg>

        <span class="font-bold text-gray-500 text-xs uppercase tracking-widest">Terminal</span>

      </div>

      {#if isRunning}

        <div class="flex items-center space-x-2 px-2 py-1 bg-green-900/20 rounded border border-green-900/30">

            <span class="relative flex h-2 w-2">

              <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-green-400 opacity-75"></span>

              <span class="relative inline-flex rounded-full h-2 w-2 bg-green-500"></span>

            </span>

            <span class="text-xs font-bold text-green-400 uppercase tracking-tight">Active</span>

        </div>

      {/if}

    </div>

    <div 

        bind:this={logContainer}

        class="flex-1 overflow-y-auto p-6 space-y-1.5 scroll-smooth custom-scrollbar"

    >

      {#each logs as log}

        <div class="leading-relaxed break-words {log.type === 'stderr' ? 'text-red-400' : log.type === 'info' ? 'text-indigo-400 font-bold' : 'text-gray-300'}">

          {log.text}

        </div>

      {/each}

      {#if logs.length === 0}

        <div class="h-full flex flex-col items-center justify-center text-gray-700 select-none">

             <svg class="w-16 h-16 mx-auto mb-4 opacity-10" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M14.752 11.168l-3.197-2.132A1 1 0 0010 9.87v4.263a1 1 0 001.555.832l3.197-2.132a1 1 0 000-1.664z"></path><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 12a9 9 0 11-18 0 9 9 0 0118 0z"></path></svg>

             <p class="text-sm font-medium opacity-40 uppercase tracking-widest">Ready to execute</p>

        </div>

      {/if}

    </div>

  </div>

</div>



<style>

  .custom-scrollbar::-webkit-scrollbar {

    width: 8px;

  }

  .custom-scrollbar::-webkit-scrollbar-track {

    background: transparent;

  }

  .custom-scrollbar::-webkit-scrollbar-thumb {

    background: #374151;

    border-radius: 4px;

  }

  .custom-scrollbar::-webkit-scrollbar-thumb:hover {

    background: #4b5563;

  }

</style>
