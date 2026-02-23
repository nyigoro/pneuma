// stealth.js — post-navigate identity normalization
// Injected after navigate when stealth_level >= 1.
// Patches JS-observable signals only. Does not affect TLS or HTTP layer.

globalThis.__pneuma_stealth_patch = function __pneuma_stealth_patch(level) {
  const applied = globalThis.__pneuma_stealth_applied || {
    level: 0,
    sessionSeed: Math.floor(Math.random() * 0xffffff),
  };
  globalThis.__pneuma_stealth_applied = applied;

  const targetLevel = Number(level || 0);
  if (targetLevel < 1) return;

  const defineSafe = (obj, key, getter, configurable = false) => {
    try {
      Object.defineProperty(obj, key, {
        get: getter,
        configurable,
      });
    } catch (_) {}
  };

  if (applied.level < 1) {
    // 1. Suppress WebDriver flag
    defineSafe(navigator, "webdriver", () => false, false);

    // 2. Normalize platform
    defineSafe(navigator, "platform", () => "Win32", false);

    // 3. Normalize hardware concurrency
    defineSafe(navigator, "hardwareConcurrency", () => 8, false);

    // 4. Normalize languages
    defineSafe(navigator, "languages", () => ["en-US", "en"], false);

    // 5. Patch plugins to return non-empty list
    defineSafe(
      navigator,
      "plugins",
      () => {
        const p = Object.create(PluginArray.prototype);
        Object.defineProperty(p, "length", { get: () => 3 });
        return p;
      },
      false
    );

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

    // 7. Stabilize canvas fingerprint with deterministic per-session suffix.
    const _origToDataURL = HTMLCanvasElement.prototype.toDataURL;
    HTMLCanvasElement.prototype.toDataURL = function (type, ...args) {
      const result = _origToDataURL.apply(this, [type, ...args]);
      return `${result}#pneuma=${applied.sessionSeed.toString(16)}`;
    };
  }

  if (targetLevel < 2) {
    applied.level = Math.max(applied.level, 1);
    return;
  }

  if (applied.level < 2) {
    // 8. Navigator vendor (Chrome persona)
    defineSafe(navigator, "vendor", () => "Google Inc.", false);

    // 9. Device memory
    defineSafe(navigator, "deviceMemory", () => 8, false);

    // 10. User agent consistency
    const ua =
      "Mozilla/5.0 (Windows NT 10.0; Win64; x64) " +
      "AppleWebKit/537.36 (KHTML, like Gecko) " +
      "Chrome/120.0.0.0 Safari/537.36";
    defineSafe(navigator, "userAgent", () => ua, false);
    defineSafe(navigator, "appVersion", () => ua.replace("Mozilla/", ""), false);

    // 11. Connection API normalization
    if ("connection" in navigator) {
      const conn = navigator.connection;
      try {
        Object.defineProperty(conn, "rtt", { get: () => 50 });
        Object.defineProperty(conn, "downlink", { get: () => 10 });
        Object.defineProperty(conn, "effectiveType", { get: () => "4g" });
        Object.defineProperty(conn, "saveData", { get: () => false });
      } catch (_) {}
    }

    // 12. WebGL renderer/vendor normalization
    const _origGetParameter = WebGLRenderingContext.prototype.getParameter;
    WebGLRenderingContext.prototype.getParameter = function (param) {
      if (param === 0x9245) return "Google Inc. (NVIDIA)";
      if (param === 0x9246) {
        return (
          "ANGLE (NVIDIA, NVIDIA GeForce RTX 3060 " +
          "Direct3D11 vs_5_0 ps_5_0, D3D11)"
        );
      }
      return _origGetParameter.call(this, param);
    };
    if (typeof WebGL2RenderingContext !== "undefined") {
      const _origGetParameter2 = WebGL2RenderingContext.prototype.getParameter;
      WebGL2RenderingContext.prototype.getParameter = function (param) {
        if (param === 0x9245) return "Google Inc. (NVIDIA)";
        if (param === 0x9246) {
          return (
            "ANGLE (NVIDIA, NVIDIA GeForce RTX 3060 " +
            "Direct3D11 vs_5_0 ps_5_0, D3D11)"
          );
        }
        return _origGetParameter2.call(this, param);
      };
    }

    // 13. Screen properties matching Windows desktop persona
    defineSafe(screen, "width", () => 1920, true);
    defineSafe(screen, "height", () => 1080, true);
    defineSafe(screen, "availWidth", () => 1920, true);
    defineSafe(screen, "availHeight", () => 1040, true);
    defineSafe(screen, "colorDepth", () => 24, true);
    defineSafe(screen, "pixelDepth", () => 24, true);

    // 14. performance.now() jitter (±0.1ms)
    const _origPerfNow = Performance.prototype.now;
    Performance.prototype.now = function () {
      const real = _origPerfNow.call(this);
      return real + (Math.random() * 0.2 - 0.1);
    };
  }

  applied.level = 2;
};
