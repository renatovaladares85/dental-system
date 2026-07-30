export function isApplicationServiceWorkerOrigin(
  location: Pick<Location, 'protocol' | 'port'>,
): boolean {
  return location.protocol === 'https:' && location.port === '8743';
}

export function registerServiceWorker(): void {
  const isApplicationOrigin = isApplicationServiceWorkerOrigin(window.location);
  if (!import.meta.env.PROD || !isApplicationOrigin || !('serviceWorker' in navigator)) {
    return;
  }

  window.addEventListener('load', () => {
    void navigator.serviceWorker
      .register('/service-worker.js', { scope: '/' })
      .catch(() => undefined);
  });
}
