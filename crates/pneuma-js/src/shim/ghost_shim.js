(function bootstrapGhostShim() {
  if (globalThis.__pneumaGhostShimLoaded) {
    return;
  }

  globalThis.__pneumaGhostShimLoaded = true;
  globalThis.__pneuma = {
    version: "0.1.0-stub",
    ready: true,
  };
})();
