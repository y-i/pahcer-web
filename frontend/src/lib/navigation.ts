export function buildTabHref(tabId: string, currentLocation: Pick<Location, 'search' | 'hash'>): string {
  const path = tabId === 'test-run' ? '/' : `/${tabId}`;
  const search = currentLocation.search;
  return `${path}${search}${currentLocation.hash}`;
}

export function navigateToTab(tabId: string): void {
  history.pushState(null, '', buildTabHref(tabId, window.location));
  window.dispatchEvent(new PopStateEvent('popstate'));
}