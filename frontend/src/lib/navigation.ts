export function buildTabHref(tabId: string, currentLocation: Pick<Location, 'search' | 'hash'>): string {
  const path = tabId === 'test-run' ? '/' : `/${tabId}`;
  return `${path}${currentLocation.search}${currentLocation.hash}`;
}

export function navigateToTab(tabId: string): void {
  history.pushState(null, '', buildTabHref(tabId, window.location));
  window.dispatchEvent(new PopStateEvent('popstate'));
}