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

<div class="h-full flex flex-col p-4 space-y-4 overflow-hidden">
  <div class="bg-white p-4 rounded-lg shadow-sm border border-gray-200 overflow-y-auto max-h-[50%]">
    <h2 class="text-lg font-semibold mb-4 text-gray-800">Pahcer Run Options</h2>
    
    <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
      <!-- Text Inputs -->
      <div class="space-y-3">
        <div>
          <label class="block text-sm font-medium text-gray-700" for="comment">Comment (-c)</label>
          <input
            id="comment"
            type="text"
            bind:value={options.comment}
            placeholder="Run comment"
            class="mt-1 block w-full px-3 py-2 border border-gray-300 rounded-md shadow-sm focus:ring-indigo-500 focus:border-indigo-500 sm:text-sm"
          />
        </div>
        <div>
          <label class="block text-sm font-medium text-gray-700" for="tag">Tag (-t)</label>
          <input
            id="tag"
            type="text"
            bind:value={options.tag}
            placeholder="Tag for the run"
            class="mt-1 block w-full px-3 py-2 border border-gray-300 rounded-md shadow-sm focus:ring-indigo-500 focus:border-indigo-500 sm:text-sm"
          />
        </div>
        <div>
          <label class="block text-sm font-medium text-gray-700" for="settingFile">Setting File</label>
          <input
            id="settingFile"
            type="text"
            bind:value={options.settingFile}
            placeholder="pahcer_config.toml"
            class="mt-1 block w-full px-3 py-2 border border-gray-300 rounded-md shadow-sm focus:ring-indigo-500 focus:border-indigo-500 sm:text-sm"
          />
        </div>
      </div>

      <!-- Checkboxes -->
      <div class="grid grid-cols-1 gap-2">
        <label class="flex items-center space-x-3 text-sm text-gray-700 cursor-pointer">
          <input type="checkbox" bind:checked={options.shuffle} class="h-4 w-4 text-indigo-600 border-gray-300 rounded focus:ring-indigo-500" />
          <span>Shuffle cases</span>
        </label>
        <label class="flex items-center space-x-3 text-sm text-gray-700 cursor-pointer">
          <input type="checkbox" bind:checked={options.json} class="h-4 w-4 text-indigo-600 border-gray-300 rounded focus:ring-indigo-500" />
          <span>JSON Output</span>
        </label>
        <label class="flex items-center space-x-3 text-sm text-gray-700 cursor-pointer">
          <input type="checkbox" bind:checked={options.freezeBestScores} class="h-4 w-4 text-indigo-600 border-gray-300 rounded focus:ring-indigo-500" />
          <span>Freeze Best Scores</span>
        </label>
        <label class="flex items-center space-x-3 text-sm text-gray-700 cursor-pointer">
          <input type="checkbox" bind:checked={options.noResultFile} class="h-4 w-4 text-indigo-600 border-gray-300 rounded focus:ring-indigo-500" />
          <span>No Result File</span>
        </label>
        <label class="flex items-center space-x-3 text-sm text-gray-700 cursor-pointer">
          <input type="checkbox" bind:checked={options.noCompile} class="h-4 w-4 text-indigo-600 border-gray-300 rounded focus:ring-indigo-500" />
          <span>No Compile</span>
        </label>
      </div>
    </div>

    <div class="mt-4 border-t pt-4">
      <label class="block text-sm font-medium text-gray-700" for="extra">Extra Arguments</label>
      <input
        id="extra"
        type="text"
        bind:value={options.extraArgs}
        placeholder="e.g. --seed 0-9"
        class="mt-1 block w-full px-3 py-2 border border-gray-300 rounded-md shadow-sm focus:ring-indigo-500 focus:border-indigo-500 sm:text-sm"
      />
    </div>

    <div class="mt-4 flex space-x-2">
      <button
        onclick={handleRun}
        disabled={isRunning}
        class="flex-1 px-4 py-2 bg-indigo-600 text-white font-semibold rounded-md hover:bg-indigo-700 disabled:opacity-50 transition-colors shadow-sm"
      >
        {isRunning ? 'Running...' : 'Run Test'}
      </button>
      <button
        onclick={clearLogs}
        class="px-4 py-2 bg-gray-200 text-gray-700 rounded-md hover:bg-gray-300 transition-colors"
      >
        Clear Console
      </button>
    </div>
  </div>

  <div class="flex-1 bg-gray-900 rounded-lg overflow-hidden flex flex-col font-mono text-sm shadow-inner">
    <div class="bg-gray-800 px-4 py-2 text-gray-400 flex justify-between items-center border-b border-gray-700">
      <span>Console Output</span>
      {#if isRunning}
        <span class="flex items-center">
            <span class="mr-2 text-xs">PROCESS RUNNING</span>
            <span class="flex h-2 w-2 rounded-full bg-green-500 animate-pulse"></span>
        </span>
      {/if}
    </div>
    <div class="flex-1 overflow-y-auto p-4 space-y-1">
      {#each logs as log}
        <div class={log.type === 'stderr' ? 'text-red-400' : log.type === 'info' ? 'text-blue-400' : 'text-gray-300'}>
          <pre class="whitespace-pre-wrap">{log.text}</pre>
        </div>
      {/each}
      {#if logs.length === 0}
        <div class="text-gray-600 italic">Console idle. Configure options and click Run Test.</div>
      {/if}
    </div>
  </div>
</div>