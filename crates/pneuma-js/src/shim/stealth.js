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

  if (targetLevel < 3) {
    applied.level = Math.max(applied.level, 2);
    return;
  }

  if (applied.level < 3) {
    // --- Level 3: deep fingerprint normalization ---
    let s = applied.sessionSeed >>> 0;
    const nextSeedByte = () => {
      s = (s * 1664525 + 1013904223) >>> 0;
      return s & 0xff;
    };
    const nextSeedFloat = () => nextSeedByte() / 255.0;

    // 15. Canvas fingerprint — deterministic per-session noise
    const _origToDataURL = HTMLCanvasElement.prototype.toDataURL;
    HTMLCanvasElement.prototype.toDataURL = function (type, quality) {
      try {
        const ctx2d = this.getContext("2d");
        if (ctx2d) {
          const imageData = ctx2d.getImageData(0, 0, 1, 1);
          imageData.data[0] =
            (imageData.data[0] + Math.floor(nextSeedFloat() * 2)) & 0xff;
          ctx2d.putImageData(imageData, 0, 0);
        }
      } catch (_) {}
      return _origToDataURL.call(this, type, quality);
    };

    if (typeof CanvasRenderingContext2D !== "undefined") {
      const _origGetImageData = CanvasRenderingContext2D.prototype.getImageData;
      CanvasRenderingContext2D.prototype.getImageData = function (x, y, w, h) {
        const data = _origGetImageData.call(this, x, y, w, h);
        for (let i = 0; i < data.data.length; i += 400) {
          data.data[i] = (data.data[i] + Math.floor(nextSeedFloat() * 2)) & 0xff;
        }
        return data;
      };
    }

    // 16. WebGL extended parameter table
    const _webglParams = {
      0x1f00: "WebGL", // VENDOR
      0x1f01: "WebGL", // RENDERER
      0x1f02: "WebGL 1.0", // VERSION
      0x8b8c: "WebGL GLSL ES 1.0", // SHADING_LANGUAGE_VERSION
      0x0b44: 1, // CULL_FACE
      0x0be2: 1, // BLEND
      0x8869: 16, // MAX_VERTEX_ATTRIBS
      0x8b4c: 16, // MAX_VERTEX_UNIFORM_VECTORS
      0x8dfb: 30, // MAX_COMBINED_TEXTURE_IMAGE_UNITS
      0x8b4d: 256, // MAX_VARYING_VECTORS
      0x8b49: 16, // MAX_FRAGMENT_UNIFORM_VECTORS
      0x0d33: 16384, // MAX_TEXTURE_SIZE
      0x851c: 16384, // MAX_CUBE_MAP_TEXTURE_SIZE
      0x8073: 16, // MAX_RENDERBUFFER_SIZE (log2)
    };

    const _patchWebGL = (proto) => {
      const _orig = proto.getParameter;
      proto.getParameter = function (param) {
        if (param === 0x9245) return "Google Inc. (NVIDIA)";
        if (param === 0x9246) {
          return (
            "ANGLE (NVIDIA, NVIDIA GeForce RTX 3060 " +
            "Direct3D11 vs_5_0 ps_5_0, D3D11)"
          );
        }
        if (param === 0x0d57) return new Int32Array([1920, 1080]);
        if (_webglParams[param] !== undefined) return _webglParams[param];
        return _orig.call(this, param);
      };

      proto.getSupportedExtensions = function () {
        return [
          "ANGLE_instanced_arrays",
          "EXT_blend_minmax",
          "EXT_color_buffer_half_float",
          "EXT_disjoint_timer_query",
          "EXT_float_blend",
          "EXT_frag_depth",
          "EXT_shader_texture_lod",
          "EXT_texture_compression_bptc",
          "EXT_texture_compression_rgtc",
          "EXT_texture_filter_anisotropic",
          "EXT_sRGB",
          "KHR_parallel_shader_compile",
          "OES_element_index_uint",
          "OES_fbo_render_mipmap",
          "OES_standard_derivatives",
          "OES_texture_float",
          "OES_texture_float_linear",
          "OES_texture_half_float",
          "OES_texture_half_float_linear",
          "OES_vertex_array_object",
          "WEBGL_color_buffer_float",
          "WEBGL_compressed_texture_s3tc",
          "WEBGL_compressed_texture_s3tc_srgb",
          "WEBGL_debug_renderer_info",
          "WEBGL_debug_shaders",
          "WEBGL_depth_texture",
          "WEBGL_draw_buffers",
          "WEBGL_lose_context",
          "WEBGL_multi_draw",
        ];
      };

      proto.getShaderPrecisionFormat = function (_shaderType, _precisionType) {
        return { rangeMin: 127, rangeMax: 127, precision: 23 };
      };
    };

    if (typeof WebGLRenderingContext !== "undefined") {
      _patchWebGL(WebGLRenderingContext.prototype);
    }
    if (typeof WebGL2RenderingContext !== "undefined") {
      _patchWebGL(WebGL2RenderingContext.prototype);
    }

    // 17. AudioContext fingerprint normalization
    if (typeof AudioContext !== "undefined" || typeof webkitAudioContext !== "undefined") {
      const AC = typeof AudioContext !== "undefined" ? AudioContext : webkitAudioContext;
      const _origCreateOscillator = AC.prototype.createOscillator;
      AC.prototype.createOscillator = function () {
        const osc = _origCreateOscillator.call(this);
        const _origConnect = osc.connect.bind(osc);
        osc.connect = function (dest) {
          return _origConnect(dest);
        };
        return osc;
      };

      const _origCreateAnalyser = AC.prototype.createAnalyser;
      AC.prototype.createAnalyser = function () {
        const analyser = _origCreateAnalyser.call(this);
        const _origGetFloat = analyser.getFloatFrequencyData.bind(analyser);
        analyser.getFloatFrequencyData = function (array) {
          _origGetFloat(array);
          for (let i = 0; i < array.length; i += 10) {
            array[i] += (nextSeedFloat() - 0.5) * 0.0001;
          }
        };

        const _origGetByte = analyser.getByteFrequencyData.bind(analyser);
        analyser.getByteFrequencyData = function (array) {
          _origGetByte(array);
          for (let i = 0; i < array.length; i += 10) {
            const delta = nextSeedByte() % 2;
            array[i] = Math.max(0, Math.min(255, array[i] + delta));
          }
        };

        return analyser;
      };
    }

    // 18. Font enumeration spoofing
    const _spoofedFonts = [
      "Arial",
      "Arial Black",
      "Arial Narrow",
      "Calibri",
      "Cambria",
      "Comic Sans MS",
      "Courier New",
      "Georgia",
      "Impact",
      "Lucida Console",
      "Lucida Sans Unicode",
      "Microsoft Sans Serif",
      "Palatino Linotype",
      "Segoe UI",
      "Tahoma",
      "Times New Roman",
      "Trebuchet MS",
      "Verdana",
      "Wingdings",
      "Wingdings 2",
      "Wingdings 3",
    ];

    if (typeof FontFaceSet !== "undefined" && document.fonts) {
      try {
        Object.defineProperty(document.fonts, "check", {
          value: function (font, _text) {
            const name = font.replace(/^[\d.]+px\s+/, "").replace(/['"]/g, "");
            return _spoofedFonts.some(
              (f) => f.toLowerCase() === name.toLowerCase()
            );
          },
          configurable: false,
        });
      } catch (_) {}

      try {
        Object.defineProperty(document.fonts, Symbol.iterator, {
          value: function () {
            const entries = _spoofedFonts.slice();
            let index = 0;
            return {
              next: function () {
                if (index >= entries.length)
                  return { done: true, value: undefined };
                const value = entries[index];
                index += 1;
                return { done: false, value };
              },
              [Symbol.iterator]: function () {
                return this;
              },
            };
          },
          configurable: false,
        });
      } catch (_) {}
    }
  }

  applied.level = 3;
};
