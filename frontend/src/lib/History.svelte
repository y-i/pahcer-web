<script lang="ts">
    import { onMount } from 'svelte';
    import { api, type ConfigResponse, type GlobalConfig, type LocalConfig } from './api';
    import { navigateToTab } from './navigation';

    let { initialConfig }: { initialConfig: ConfigResponse } = $props();

    let historyData = $state<any[]>([]);
    let isLoading = $state(true);
    let error = $state('');
    let selectedRow = $state<any | null>(null);
    let expandedRows = $state(new Set<string>());
    let deletingIds = $state(new Set<string>());

    let visualizerUrl = $state('/visualizer/index.html');
    let config = $state<GlobalConfig>({
        visualizerPosition: 'right',
        visualizerInitialScrollPosition: 'bottom',
        visualizerUrl: '',
        resultJsonMode: 'symlink',
        defaultSeed: 0,
        defaultScale: 1.0
    });
    let localConfig = $state<LocalConfig>({
        visualizerUrl: ''
    });

    let seed = $state(0);
    let scalePercent = $state(100);
    let urlSyncPhase = $state<'loading' | 'restoring' | 'ready'>('loading');
    let visualizerStatus = $state<'idle' | 'checking' | 'ready' | 'missing'>('idle');
    let visualizerStatusError = $state('');
    let visualizerCheckToken = 0;

    let iframeSrc = $derived.by(() => {
        if (!selectedRow) return '';
        const filename = String(seed).padStart(4, '0') + '.txt';
        const outputUrl = encodeURIComponent(`/api/history/${selectedRow.id}/output/${filename}`);
        return `${visualizerUrl}?output_url=${outputUrl}&seed=${seed}&initial_scroll=${config.visualizerInitialScrollPosition}`;
    });

    onMount(() => {
        window.addEventListener('popstate', restoreSelectionFromUrl);

        (async () => {
            try {
                const historyRes = await api.getHistory();
                historyData = historyRes;
                config = initialConfig.global;
                localConfig = { ...localConfig, ...initialConfig.local };
                resetVisualizerControls();
                restoreSelectionFromUrl();
            } catch (e) {
                error = 'Failed to load data';
                console.error(e);
            } finally {
                isLoading = false;
            }
        })();

        return () => {
            window.removeEventListener('popstate', restoreSelectionFromUrl);
        };
    });

    function getDefaultScalePercent() {
        return Math.round(config.defaultScale * 100);
    }

    function resetVisualizerControls() {
        seed = config.defaultSeed;
        scalePercent = getDefaultScalePercent();
    }

    function parseNumericParam(value: string | null, fallback: number) {
        if (value === null || value.trim() === '') {
            return fallback;
        }

        const parsed = Number(value);
        return Number.isFinite(parsed) ? parsed : fallback;
    }

    function markUrlSyncReady() {
        setTimeout(() => {
            urlSyncPhase = 'ready';
        }, 0);
    }

    function restoreSelectionFromUrl() {
        urlSyncPhase = 'restoring';

        const params = new URLSearchParams(window.location.search);
        const id = params.get('id');

        if (!id) {
            selectedRow = null;
            resetVisualizerControls();
            markUrlSyncReady();
            return;
        }

        const row = historyData.find((historyRow) => historyRow.id === id);
        if (!row) {
            selectedRow = null;
            resetVisualizerControls();
            markUrlSyncReady();
            return;
        }

        selectedRow = row;
        seed = parseNumericParam(params.get('seed'), config.defaultSeed);
        scalePercent = parseNumericParam(params.get('scale'), getDefaultScalePercent());
        markUrlSyncReady();
    }

    function selectRow(row: any) {
        if (selectedRow?.id === row.id) {
            return;
        }

        selectedRow = row;
        resetVisualizerControls();
    }

    function closeVisualizer() {
        if (!selectedRow) {
            return;
        }

        selectedRow = null;
        resetVisualizerControls();
    }

    function goToProjectVisualizerSettings() {
        navigateToTab('settings');
        requestAnimationFrame(() => {
            requestAnimationFrame(() => {
                const input = document.getElementById('project-visualizer-url');
                if (input instanceof HTMLInputElement) {
                    input.scrollIntoView({ behavior: 'smooth', block: 'center' });
                    input.focus();
                }
            });
        });
    }

    function toggleDetails(row: any, event: Event) {
        event.stopPropagation();
        const newSet = new Set(expandedRows);
        if (newSet.has(row.id)) {
            newSet.delete(row.id);
        } else {
            newSet.add(row.id);
        }
        expandedRows = newSet;
    }

    async function deleteRow(id: string, event: Event) {
        event.stopPropagation();
        if (!window.confirm('Are you sure you want to delete this execution result?')) return;

        deletingIds.add(id);
        try {
            await api.deleteHistory(id);
            historyData = historyData.filter(r => r.id !== id);
            if (selectedRow?.id === id) {
                selectedRow = null;
            }
        } catch (e) {
            console.error('Failed to delete history:', e);
            alert('Failed to delete history');
        } finally {
            deletingIds.delete(id);
        }
    }

    $effect(() => {
        if (urlSyncPhase !== 'ready') return;

        const url = new URL(window.location.href);
        const currentId = url.searchParams.get('id');
        const currentSeed = url.searchParams.get('seed');
        const currentScale = url.searchParams.get('scale');

        if (!selectedRow) {
            if (currentId || currentSeed || currentScale) {
                url.searchParams.delete('id');
                url.searchParams.delete('seed');
                url.searchParams.delete('scale');
                history.pushState(null, '', url.toString());
            }
            return;
        }

        const newId = selectedRow.id;
        const newSeed = String(seed);
        const newScale = String(scalePercent);

        if (currentId !== newId) {
            url.searchParams.set('id', newId);
            url.searchParams.set('seed', newSeed);
            url.searchParams.set('scale', newScale);
            history.pushState(null, '', url.toString());
        } else if (currentSeed !== newSeed || currentScale !== newScale) {
            url.searchParams.set('id', newId);
            url.searchParams.set('seed', newSeed);
            url.searchParams.set('scale', newScale);
            history.replaceState(null, '', url.toString());
        }
    });

    $effect(() => {
        const selectedRowId = selectedRow?.id;

        if (!selectedRowId) {
            visualizerStatus = 'idle';
            visualizerStatusError = '';
            return;
        }

        const currentToken = ++visualizerCheckToken;
        visualizerStatus = 'checking';
        visualizerStatusError = '';

        void (async () => {
            try {
                const { exists } = await api.getVisualizerStatus();
                if (currentToken !== visualizerCheckToken) {
                    return;
                }
                visualizerStatus = exists ? 'ready' : 'missing';
            } catch (e) {
                if (currentToken !== visualizerCheckToken) {
                    return;
                }
                visualizerStatus = 'missing';
                visualizerStatusError = e instanceof Error ? e.message : 'Failed to check visualizer status';
            }
        })();
    });

        return `${intPart}.${parts[1]}%`;
    }

    function formatTime(n: number) {
        return Math.round(n * 1000).toLocaleString();
    }

    function getACCount(row: any) {
        if (row.ACcase !== undefined) {
            return row.ACcase;
        }
        if (row.details && Array.isArray(row.details)) {
            return row.details.filter((r: any) => (Number(r.score) || 0) > 0 && !r.error_message).length;
        }
    function formatDate(iso: string) {
        const d = new Date(iso);
        const padjw = (n: number) => n.toString().padStart(2, '0');
        return `${d.getFullYear()}/${padjw(d.getMonth() + 1)}/${padjw(d.getDate())} ${padjw(d.getHours())}:${padjw(d.getMinutes())}:${padjw(d.getSeconds())}`;
    }

    function usesScientificScoreDisplay() {
        return localConfig.historyScoreDisplayFormat === 'scientific';
    }

    function formatHistoryScore(n: number) {
        const rounded = Math.round(n);
        if (usesScientificScoreDisplay()) {
            if (rounded === 0) {
                return '0';
            }
            return rounded.toExponential(2);
        }
        return rounded.toLocaleString();
    }

    function formatRelative(n: number) {
        const s = n.toFixed(4);
        const parts = s.split('.');
        const intPart = parts[0].padStart(4, ' ');
        return 0;
    }
</script>

<div class="h-full flex flex-col bg-gray-50 overflow-hidden min-h-0">
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

    function hasProjectVisualizerUrl() {
        return (localConfig.visualizerUrl ?? '').trim().length > 0;
    }
    <div class="flex-1 flex overflow-hidden p-6 max-w-none w-full min-h-0">
      <!-- List View -->
      <div class="{selectedRow ? 'w-[calc(50%-0.75rem)]' : 'w-full'} 
                  {config.visualizerPosition === 'left' ? 'order-2' : 'order-1'} 
                  flex flex-col bg-white rounded-xl shadow-sm border border-gray-200 overflow-hidden transition-all duration-300 z-0 min-h-0">
        <div class="px-6 py-4 border-b border-gray-200 bg-white flex justify-between items-center flex-shrink-0 z-10">
            <h2 class="text-lg font-bold text-gray-900 tracking-tight">Execution History</h2>
            <button 
                onclick={() => api.getHistory().then(res => historyData = res)} 
                class="inline-flex items-center px-3 py-1.5 border border-transparent text-xs font-medium rounded-md text-indigo-700 bg-indigo-100 hover:bg-indigo-200 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-indigo-500 transition-colors"
            >
                Refresh
            </button>
        </div>
        <div class="flex-1 overflow-auto min-h-0">
            <table class="min-w-full divide-y divide-gray-200">
                <thead class="bg-gray-50 sticky top-0 z-10 shadow-sm">
                    <tr>
                        <th scope="col" class="px-3 py-3 w-8 bg-gray-50"></th>
                        <th scope="col" class="px-6 py-3 text-left text-xs font-semibold text-gray-500 uppercase tracking-wider whitespace-nowrap bg-gray-50">Date</th>
                        <th scope="col" class="px-6 py-3 text-right text-xs font-semibold text-gray-500 uppercase tracking-wider whitespace-nowrap bg-gray-50">AC/Cases</th>
                        <th scope="col" class="px-6 py-3 text-right text-xs font-semibold text-gray-500 uppercase tracking-wider whitespace-nowrap bg-gray-50">Avg Score</th>
                        <th scope="col" class="px-6 py-3 text-right text-xs font-semibold text-gray-500 uppercase tracking-wider whitespace-nowrap bg-gray-50">Avg Log</th>
                        <th scope="col" class="px-6 py-3 text-right text-xs font-semibold text-gray-500 uppercase tracking-wider whitespace-nowrap bg-gray-50">Avg Rel</th>
                        <th scope="col" class="px-6 py-3 text-right text-xs font-semibold text-gray-500 uppercase tracking-wider whitespace-nowrap bg-gray-50">Max Time</th>
                        <th scope="col" class="px-6 py-3 text-left text-xs font-semibold text-gray-500 uppercase tracking-wider whitespace-nowrap bg-gray-50">Comment</th>
                        <th scope="col" class="px-6 py-3 text-left text-xs font-semibold text-gray-500 uppercase tracking-wider whitespace-nowrap bg-gray-50">Tag</th>
                        <th scope="col" class="px-6 py-3 text-left text-xs font-semibold text-gray-500 uppercase tracking-wider whitespace-nowrap bg-gray-50"></th>
                    </tr>
                </thead>
                <tbody class="bg-white divide-y divide-gray-200">
                    {#each historyData as row (row.id)}
                        <tr 
                            class="group hover:bg-indigo-50/50 cursor-pointer transition-colors duration-150 ease-in-out {selectedRow === row ? 'bg-indigo-50' : ''}"
                            onclick={() => selectRow(row)}
                        >
                            <td class="px-3 py-4 whitespace-nowrap text-sm text-gray-500">
                                <button 
                                    class="p-1 rounded-full hover:bg-gray-200 transition-colors focus:outline-none"
                                    onclick={(e) => toggleDetails(row, e)}
                                    title={expandedRows.has(row.id) ? "Collapse details" : "Expand details"}
                                >
                                    {#if expandedRows.has(row.id)}
                                        <svg class="w-4 h-4 text-gray-500" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 15l7-7 7 7"></path></svg>
                                    {:else}
                                        <svg class="w-4 h-4 text-gray-500" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7"></path></svg>
                                    {/if}
                                </button>
                            </td>
                            <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-700 font-mono">{formatDate(row.datetime)}</td>
                            <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-700 font-mono text-right">{getACCount(row)}/{row.cases}</td>
                            <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-900 font-mono font-medium text-right">{formatHistoryScore(row.avgScore)}</td>
                            <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-700 font-mono text-right">{row.avgLogScore.toFixed(3)}</td>
                            <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-700 font-mono text-right whitespace-pre">{row.avgRelativeScore !== undefined ? formatRelative(row.avgRelativeScore) : '-'}</td>
                            <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-700 font-mono text-right">{Math.round(row.maxTime)}ms</td>
                            <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-500">{row.comment || '-'}</td>
                            <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-500">
                                {#if row.tag}
                                    <span class="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-gray-100 text-gray-800">
                                        {row.tag}
                                    </span>
                                {:else}
                                    -
                                {/if}
                            </td>
                            <td class="px-6 py-4 whitespace-nowrap text-right text-sm font-medium">
                                <button 
                                    onclick={(e) => deleteRow(row.id, e)}
                                    class="text-gray-400 hover:text-red-600 transition-colors p-1 rounded-md hover:bg-red-50 disabled:opacity-50"
                                    title="Delete result"
                                    disabled={deletingIds.has(row.id)}
                                >
                                    {#if deletingIds.has(row.id)}
                                        <svg class="animate-spin h-4 w-4" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                                            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                                            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                                        </svg>
                                    {:else}
                                        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                                        </svg>
                                    {/if}
                                </button>
                            </td>
                        </tr>
                        {#if expandedRows.has(row.id)}
                            <tr class="bg-gray-50/50 cursor-default">
                                <td colspan="10" class="px-6 py-4">
                                    <div class="overflow-hidden rounded-lg border border-gray-200 bg-white shadow-inner">
                                        {#if row.details && row.details.length > 0}
                                            <div class="max-h-96 overflow-y-auto">
                                                <table class="min-w-full divide-y divide-gray-200">
                                                    <thead class="bg-gray-100 sticky top-0">
                                                        <tr>
                                                            <th scope="col" class="px-4 py-2 text-left text-xs font-medium text-gray-500 uppercase tracking-wider w-24">Seed</th>
                                                            <th scope="col" class="px-4 py-2 text-right text-xs font-medium text-gray-500 uppercase tracking-wider w-32">Score</th>
                                                            <th scope="col" class="px-4 py-2 text-right text-xs font-medium text-gray-500 uppercase tracking-wider w-32">Relative Score</th>
                                                            <th scope="col" class="px-4 py-2 text-right text-xs font-medium text-gray-500 uppercase tracking-wider w-32">Time (ms)</th>
                                                            <th scope="col" class="px-4 py-2 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">Error</th>
                                                            <th scope="col" class="px-4 py-2"></th>
                                                        </tr>
                                                    </thead>
                                                    <tbody class="divide-y divide-gray-200 bg-white">
                                                        {#each row.details.slice().sort((a: any, b: any) => (Number(a.seed) || 0) - (Number(b.seed) || 0)) as detail (`${row.id}-${detail.seed}`)}
                                                            <tr class="hover:bg-gray-50">
                                                                <td class="px-4 py-2 whitespace-nowrap text-sm text-gray-900 font-mono">{detail.seed}</td>
                                                                <td class="px-4 py-2 whitespace-nowrap text-sm text-gray-900 font-mono font-medium text-right">{formatHistoryScore(Number(detail.score) || 0)}</td>
                                                                <td class="px-4 py-2 whitespace-nowrap text-sm text-gray-500 font-mono text-right whitespace-pre">{detail.relative_score !== undefined ? formatRelative(Number(detail.relative_score)) : '-'}</td>
                                                                <td class="px-4 py-2 whitespace-nowrap text-sm text-gray-500 font-mono text-right">{detail.execution_time !== undefined ? formatTime(Number(detail.execution_time)) : (detail.time !== undefined ? formatTime(Number(detail.time)) : '-')}</td>
                                                                <td class="px-4 py-2 text-sm text-red-600 font-mono">{detail.error_message || ''}</td>
                                                                <td class="px-4 py-2"></td>
                                                            </tr>
                                                        {/each}
                                                    </tbody>
                                                </table>
                                            </div>
                                        {:else}
                                            <div class="p-4 text-center text-sm text-gray-500">No detailed results available.</div>
                                        {/if}
                                    </div>
                                </td>
                            </tr>
                        {/if}
                    {/each}
                </tbody>
            </table>
            {#if historyData.length === 0}
                <div class="flex flex-col items-center justify-center p-12 text-center">
                    <div class="rounded-full bg-gray-100 p-3 mb-4">
                        <svg class="h-6 w-6 text-gray-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2" />
                        </svg>
                    </div>
                    <h3 class="text-sm font-medium text-gray-900">No history found</h3>
                    <p class="mt-1 text-sm text-gray-500">Run a test to generate results.</p>
                </div>
            {/if}
        </div>
      </div>
      <!-- Visualizer View -->
      <div class="{selectedRow ? 'w-[calc(50%-0.75rem)] opacity-100' : 'w-0 opacity-0 border-0 p-0 overflow-hidden'} 
                  {config.visualizerPosition === 'left' ? 'order-1' : 'order-2'}
                  {config.visualizerPosition === 'left' && selectedRow ? 'mr-6' : ''}
                  {config.visualizerPosition === 'right' && selectedRow ? 'ml-6' : ''}
                  flex flex-col bg-white rounded-xl shadow-sm border border-gray-200 overflow-hidden z-20 transition-all duration-300 min-h-0">
        {#if selectedRow}
            <div class="px-4 py-3 border-b border-gray-200 bg-white flex items-center space-x-6 shadow-sm z-10 min-w-0 flex-shrink-0">
                <span class="text-xs font-bold text-gray-400 uppercase tracking-wider flex-shrink-0">Visualizer</span>
                {#if visualizerStatus === 'ready'}
                    <div class="h-4 w-px bg-gray-300 flex-shrink-0"></div>
                    <div class="flex items-center space-x-3 min-w-0">
                        <label class="text-sm font-medium text-gray-600 whitespace-nowrap" for="history-visualizer-seed">Seed</label>
                        <input 
                            id="history-visualizer-seed"
                            type="number" 
                            bind:value={seed} 
                            class="w-20 px-2 py-1 bg-gray-50 border border-gray-300 rounded text-sm focus:ring-1 focus:ring-indigo-500 focus:border-indigo-500" 
                        />
                    </div>
                    <div class="flex items-center space-x-3 min-w-0">
                        <label class="text-sm font-medium text-gray-600 whitespace-nowrap" for="history-visualizer-scale">Scale (%)</label>
                        <input 
                            id="history-visualizer-scale"
                            type="number" 
                            step="5"
                            bind:value={scalePercent} 
                            class="w-16 px-2 py-1 bg-gray-50 border border-gray-300 rounded text-sm focus:ring-1 focus:ring-indigo-500 focus:border-indigo-500" 
                        />
                    </div>
                {:else if visualizerStatus === 'checking'}
                    <div class="inline-flex items-center gap-2 rounded-full bg-indigo-50 px-3 py-1 text-xs font-medium text-indigo-700">
                        <svg class="h-3.5 w-3.5 animate-spin" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                        </svg>
                        <span>ビジュアライザを確認中</span>
                    </div>
                {:else if visualizerStatus === 'missing'}
                    <span class="inline-flex items-center rounded-full bg-amber-100 px-3 py-1 text-xs font-medium text-amber-800">Project Settings の確認が必要です</span>
                {/if}
                <div class="flex-1"></div>
                <button 
                    onclick={closeVisualizer} 
                    class="p-1 rounded-md text-gray-400 hover:text-gray-600 hover:bg-gray-100 transition-colors flex-shrink-0"
                    title="Close Visualizer"
                >
                    <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"></path></svg>
                </button>
            </div>
            <div class="flex-1 relative bg-gray-50 min-w-0 overflow-hidden">
                {#if visualizerStatus === 'ready'}
                    <iframe 
                        title="Visualizer"
                        src={iframeSrc} 
                        class="border-none"
                        style="width: {10000 / scalePercent}%; height: {10000 / scalePercent}%; transform: scale({scalePercent / 100}); transform-origin: 0 0;"
                    ></iframe>
                {:else if visualizerStatus === 'checking'}
                    <div class="flex h-full items-center justify-center">
                        <div class="inline-flex items-center gap-3 rounded-2xl border border-gray-200 bg-white px-5 py-4 text-sm text-gray-600 shadow-sm">
                            <svg class="h-5 w-5 animate-spin text-indigo-600" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                                <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                            </svg>
                            <span>ビジュアライザファイルの有無を確認しています...</span>
                        </div>
                    </div>
                {:else}
                    <div class="flex h-full items-center justify-center p-6">
                        <div class="w-full max-w-lg rounded-3xl border border-amber-200 bg-white p-8 text-center shadow-lg shadow-amber-100/50">
                            <div class="mx-auto mb-5 flex h-14 w-14 items-center justify-center rounded-full bg-amber-100 text-amber-600">
                                <svg class="h-7 w-7" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
                                </svg>
                            </div>
                            {#if hasProjectVisualizerUrl()}
                                <h3 class="text-lg font-bold text-gray-900">ビジュアライザファイルがまだ配置されていません</h3>
                                <p class="mt-3 text-sm leading-6 text-gray-600">
                                    Project Settings の Project Visualizer URL は設定済みですが、ローカルのビジュアライザファイルが見つかりません。
                                    設定タブで URL を確認して Project Settings を保存し直すと、visualizer を再ダウンロードできます。
                                </p>
                            {:else}
                                <h3 class="text-lg font-bold text-gray-900">Project Settings にビジュアライザ URL を設定してください</h3>
                                <p class="mt-3 text-sm leading-6 text-gray-600">
                                    評価履歴から visualizer を開くには、Project Settings の Project Visualizer URL を入力して保存し、
                                    visualizer 本体をダウンロードする必要があります。
                                </p>
                            {/if}
                            {#if visualizerStatusError}
                                <p class="mt-4 rounded-2xl bg-amber-50 px-4 py-3 text-left text-xs text-amber-800">{visualizerStatusError}</p>
                            {/if}
                            <div class="mt-6 flex justify-center">
                                <button
                                    onclick={goToProjectVisualizerSettings}
                                    class="inline-flex items-center rounded-xl bg-indigo-600 px-5 py-2.5 text-sm font-semibold text-white transition hover:bg-indigo-700"
                                >
                                    設定へ移動
                                </button>
                            </div>
                        </div>
                    </div>
                {/if}
            </div>
        {/if}
      </div>
    </div>
  {/if}
</div>