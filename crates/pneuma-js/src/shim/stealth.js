// stealth.js — post-navigate identity normalization
// Injected after navigate when stealth_level >= 1.
// Patches JS-observable signals only. Does not affect TLS or HTTP layer.

globalThis.__pneuma_stealth_patch = function __pneuma_stealth_patch() {
  // 1. Suppress WebDriver flag
  Object.defineProperty(navigator, "webdriver", {
    get: () => false,
    configurable: false,
  });

  // 2. Normalize platform
  Object.defineProperty(navigator, "platform", {
    get: () => "Win32",
    configurable: false,
  });

  // 3. Normalize hardware concurrency (8 is common for mid-range machines)
  Object.defineProperty(navigator, "hardwareConcurrency", {
    get: () => 8,
    configurable: false,
  });

  // 4. Normalize languages
  Object.defineProperty(navigator, "languages", {
    get: () => ["en-US", "en"],
    configurable: false,
  });

  // 5. Patch plugins to return non-empty list
  Object.defineProperty(navigator, "plugins", {
    get: () => {
      const p = Object.create(PluginArray.prototype);
      Object.defineProperty(p, "length", { get: () => 3 });
      return p;
    },
    configurable: false,
  });

  // 6. Remove WebDriver artifact globals
  const artifacts = [
    "cdc_adoQpoasnfa76pfcZLmcfl_Array",
    "cdc_adoQpoasnfa76pfcZLmcfl_Promise",
    "cdc_adoQpoasnfa76pfcZLmcfl_Symbol",
    "__webdriver_script_fn",
    "__driver_evaluate",
    "__webdriver_evaluate",
    "__selenium_evaluate",
    "__fxdriver_evaluate",
  ];
  for (const key of artifacts) {
    try {
      delete window[key];
    } catch (_) {}
  }

  // 7. Stabilize canvas fingerprint
  // Add a deterministic fragment to the data URL so hashes differ
  // while remaining stable for the session.
  const _sessionSeed = Math.floor(Math.random() * 0xffffff);
  const _origToDataURL = HTMLCanvasElement.prototype.toDataURL;
  HTMLCanvasElement.prototype.toDataURL = function (type, ...args) {
    const result = _origToDataURL.apply(this, [type, ...args]);
    return `${result}#pneuma=${_sessionSeed.toString(16)}`;
  };
};
