<script lang="ts">
  import { api } from './api';

  let args = $state('');
  let logs = $state<{ type: 'stdout' | 'stderr' | 'info'; text: string }[]>([]);
  let isRunning = $state(false);

  async function handleRun() {
    if (isRunning) return;
    
    isRunning = true;
    logs = [{ type: 'info', text: 'Starting pahcer run...' }];
    
    const argList = args.split(/\s+/).filter(a => a.length > 0);
    
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

<div class="h-full flex flex-col p-4 space-y-4">
  <div class="bg-white p-4 rounded-lg shadow-sm border border-gray-200">
    <h2 class="text-lg font-semibold mb-4 text-gray-800">Pahcer Run</h2>
    <div class="flex space-x-2">
      <input
        type="text"
        bind:value={args}
        placeholder="e.g. -c default -s 0-9"
        class="flex-1 px-3 py-2 border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-indigo-500"
        disabled={isRunning}
      />
      <button
        onclick={handleRun}
        disabled={isRunning}
        class="px-4 py-2 bg-indigo-600 text-white rounded-md hover:bg-indigo-700 disabled:opacity-50 transition-colors"
      >
        {isRunning ? 'Running...' : 'Run'}
      </button>
      <button
        onclick={clearLogs}
        class="px-4 py-2 bg-gray-200 text-gray-700 rounded-md hover:bg-gray-300 transition-colors"
      >
        Clear
      </button>
    </div>
  </div>

  <div class="flex-1 bg-gray-900 rounded-lg overflow-hidden flex flex-col font-mono text-sm">
    <div class="bg-gray-800 px-4 py-2 text-gray-400 flex justify-between items-center">
      <span>Console Output</span>
      {#if isRunning}
        <span class="flex h-2 w-2 rounded-full bg-green-500 animate-pulse"></span>
      {/if}
    </div>
    <div class="flex-1 overflow-y-auto p-4 space-y-1">
      {#each logs as log}
        <div class={log.type === 'stderr' ? 'text-red-400' : log.type === 'info' ? 'text-blue-400' : 'text-gray-300'}>
          <pre class="whitespace-pre-wrap">{log.text}</pre>
        </div>
      {/each}
      {#if logs.length === 0}
        <div class="text-gray-600 italic">No output yet. Enter arguments and click Run.</div>
      {/if}
    </div>
  </div>
</div>
