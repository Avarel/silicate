const CACHE_NAME = 'silicate-pwa';
const CACHE_EXPIRATION_TIME = 24 * 60 * 60 * 1000; // 24 hours

var filesToCache = [
  './',
  './index.html',
  './silicate.js',
  './silicate_bg.wasm',
];

/* Start the service worker and cache all of the app's content */
self.addEventListener('install', function (e) {
  e.waitUntil(
    caches.open(CACHE_NAME).then(function (cache) {
      return cache.addAll(filesToCache);
    })
  );
});

/* Serve */
self.addEventListener('fetch', (event) => {
  event.respondWith(
    caches.open(CACHE_NAME).then(async (cache) => {
      const cachedResponse = await cache.match(event.request);
      const now = Date.now();
      if (cachedResponse) {
        const cachedTime = cachedResponse.headers.get('sw-cache-time');
        if (now - cachedTime < CACHE_EXPIRATION_TIME) {
          return cachedResponse;
        }
      }
      const networkResponse = await fetch(event.request);
      // if response invalid, return cached response if available
      if (!networkResponse || networkResponse.status !== 200) {
        return cachedResponse || networkResponse;
      }

      // Clone the response and add the timestamp header before caching
      const responseWithTimestamp = networkResponse.clone();
      let headers = new Headers(responseWithTimestamp.headers);
      headers.append('sw-cache-time', now.toString());
      responseWithTimestamp.headers = headers;

      cache.put(event.request, responseWithTimestamp);
      return networkResponse;
    })
  );
});
