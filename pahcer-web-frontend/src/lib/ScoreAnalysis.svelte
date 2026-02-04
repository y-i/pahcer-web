<script lang="ts">
  let iframeSrc = $state('/analysis/index.html');
  let isLoading = $state(false);

  async function reloadAnalysis() {
    isLoading = true;
    try {
        // Trigger download API to ensure latest version or fix if missing
        await fetch('/api/analysis/download', {
            method: 'POST'
        });
        
        // Force reload by updating query param
        iframeSrc = `/analysis/index.html?t=${Date.now()}`;
    } catch (e) {
        console.error(e);
        alert('Failed to reload analysis tool.');
    } finally {
        isLoading = false;
    }
  }
</script>

<div class="h-full flex flex-col relative bg-gray-50">
  <div class="absolute top-2 right-4 z-10">
      <button 
        onclick={reloadAnalysis}
        class="bg-white/90 hover:bg-white text-gray-700 px-3 py-1 rounded shadow text-sm border border-gray-300 transition-colors"
        disabled={isLoading}
      >
        {isLoading ? 'Reloading...' : 'Reload Tool'}
      </button>
  </div>
  <iframe
    src={iframeSrc}
    title="Score Analysis"
    class="w-full flex-grow border-none"
  ></iframe>
</div>
