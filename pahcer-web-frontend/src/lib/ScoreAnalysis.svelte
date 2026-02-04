<script lang="ts">
  let iframeSrc = $state('/analysis/index.html');
  let isLoading = $state(false);

  async function reloadAnalysis() {
    isLoading = true;
    try {
        await fetch('/api/analysis/download', {
            method: 'POST'
        });
        iframeSrc = `/analysis/index.html?t=${Date.now()}`;
    } catch (e) {
        console.error(e);
        alert('Failed to reload analysis tool.');
    } finally {
        isLoading = false;
    }
  }
</script>

<div class="h-full flex flex-col bg-gray-50 p-6">
  <div class="flex-1 flex flex-col relative bg-white rounded-xl shadow-sm border border-gray-200 overflow-hidden">
    <div class="absolute top-3 right-4 z-10">
        <button 
          onclick={reloadAnalysis}
          class="bg-white/90 hover:bg-white text-gray-700 px-3 py-1.5 rounded-lg shadow-sm text-sm border border-gray-300 transition-all active:scale-95 disabled:opacity-50"
          disabled={isLoading}
        >
          {isLoading ? 'Reloading...' : 'Reload Tool'}
        </button>
    </div>
    <iframe
      src={iframeSrc}
      title="Score Analysis"
      class="w-full h-full border-none"
    ></iframe>
  </div>
</div>
