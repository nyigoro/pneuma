(function bootstrapGhostShim() {
  if (globalThis.__pneumaGhostShimLoaded) return;
  globalThis.__pneumaGhostShimLoaded = true;

  const ffi = globalThis.__pneuma_private_ffi;
  if (!ffi) {
    throw new Error(
      "[Pneuma] FATAL: __pneuma_private_ffi not found. " +
      "FFI bridge must be registered before ghost_shim.js is evaluated."
    );
  }

  class ElementHandle {
    constructor(page, selector) {
      this._page = page;
      this._selector = selector;
    }

    async click() {
      return this._page.evaluate(
        (sel) => document.querySelector(sel)?.click(),
        this._selector
      );
    }

    async textContent() {
      return this._page.evaluate(
        (sel) => document.querySelector(sel)?.textContent ?? null,
        this._selector
      );
    }
  }

  class Page {
    constructor(id) {
      this._id = id;
    }

    async goto(url, options = {}) {
      const raw = ffi.navigate(this._id, url, JSON.stringify(options));
      const meta = JSON.parse(raw);
      if (meta.error) throw new Error(`Navigation failed: ${meta.error}`);
      return meta;
    }

    async evaluate(fn, ...args) {
      const script = `(${fn.toString()})(${args.map(JSON.stringify).join(",")})`;
      const raw = ffi.evaluate(this._id, script);
      return JSON.parse(raw);
    }

    async $(selector) {
      const exists = await this.evaluate(
        (sel) => !!document.querySelector(sel),
        selector
      );
      return exists ? new ElementHandle(this, selector) : null;
    }

    async screenshot(options = {}) {
      return ffi.screenshot(this._id);
    }

    async title() {
      return this.evaluate(() => document.title);
    }

    async content() {
      return this.evaluate(() => document.documentElement.outerHTML);
    }
  }

  class Browser {
    async newPage() {
      const id = ffi.createPage();
      return new Page(id);
    }

    async close() {
      ffi.closeBrowser();
    }
  }

  globalThis.ghost = {
    version: "0.1.0",

    launch: async (options = {}) => {
      return new Browser(options);
    },

    open: async (url, options = {}) => {
      const browser = new Browser();
      const page = await browser.newPage();
      await page.goto(url, options);
      return page;
    },

    exit: (code = 0) => ffi.exit(code),
  };

  globalThis.console = {
    log: (...args) => {
      ffi.log("info", args.map(String).join(" "));
    },
    warn: (...args) => {
      ffi.log("warn", args.map(String).join(" "));
    },
    error: (...args) => {
      ffi.log("error", args.map(String).join(" "));
    },
    debug: (...args) => {
      ffi.log("info", args.map(String).join(" "));
    },
  };

  globalThis.__pneuma = {
    version: "0.1.0",
    ready: true,
    ffi_connected: true,
  };
})();
