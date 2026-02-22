// bridge.cpp — C-ABI bridge: Rust <-> Ladybird HeadlessWebView
//
// Threading contract: all Ladybird objects are created and used exclusively
// on the dedicated OS thread managed by LadybirdHandle in lib.rs.
// Core::EventLoop::current() returns the loop bound to that thread.

#include <AK/ByteString.h>
#include <AK/LexicalPath.h>
#include <LibCore/EventLoop.h>
#include <LibGfx/SystemTheme.h>
#include <LibMain/Main.h>
#include <LibURL/Parser.h>
#include <LibURL/URL.h>
#include <LibWeb/PixelUnits.h>
#include <LibWebView/Application.h>
#include <LibWebView/HeadlessWebView.h>
#include <LibWebView/Options.h>
#include <LibWebView/Utilities.h>

#include <chrono>
#include <cstdio>
#include <cstdlib>
#include <cstring>

// ---------------------------------------------------------------------------
// Status codes (must match lib.rs)
// ---------------------------------------------------------------------------
static constexpr int PNEUMA_OK = 0;
static constexpr int PNEUMA_INVALID_ARG = 1;
static constexpr int PNEUMA_TIMEOUT = 2;
static constexpr int PNEUMA_RUNTIME_ERR = 3;

class PneumaLadybirdApplication final : public WebView::Application {
    WEB_VIEW_APPLICATION(PneumaLadybirdApplication)

private:
    explicit PneumaLadybirdApplication() = default;
};

// ---------------------------------------------------------------------------
// Opaque browser state — one instance per dedicated thread
// ---------------------------------------------------------------------------
struct PneumaLadybirdBrowser {
    OwnPtr<PneumaLadybirdApplication> app;

    // Stable storage for Main::Arguments.
    // ArgsParser::parse() asserts arguments.strings is non-empty.
    ByteString arg0_storage;
    char* argv_storage[1];
    StringView strings_storage[1];

    OwnPtr<WebView::HeadlessWebView> view;

    // Load state — written from event loop callbacks, read from pump loop.
    // Single-threaded: no atomics needed, volatile prevents optimizer elision.
    volatile bool load_complete { false };
    volatile bool load_failed { false };
    volatile bool title_seen { false };
    ByteString last_title_utf8;
    ByteString last_error;
};

// ---------------------------------------------------------------------------
// C-ABI exports
// ---------------------------------------------------------------------------

extern "C" PneumaLadybirdBrowser*
pneuma_ladybird_browser_create(int width, int height)
{
    auto* browser = new (std::nothrow) PneumaLadybirdBrowser();
    if (!browser)
        return nullptr;

    // Build non-empty Main::Arguments with program name.
    browser->arg0_storage = ByteString("pneuma-ladybird");
    browser->argv_storage[0] = const_cast<char*>(browser->arg0_storage.characters());
    browser->strings_storage[0] = StringView(browser->arg0_storage);

    Main::Arguments arguments {
        .argc = 1,
        .argv = browser->argv_storage,
        .strings = Span<StringView>(browser->strings_storage, 1),
    };

    // Initialize Application — creates the event loop and launches services.
    auto app_result = PneumaLadybirdApplication::create(arguments);
    if (app_result.is_error()) {
        // Avoid passing StringView to %s directly; convert explicitly.
        ByteString msg = app_result.error().string_literal();
        fprintf(stderr, "[pneuma-ladybird] Application::initialize failed: %s\n",
            msg.characters());
        delete browser;
        return nullptr;
    }
    browser->app = app_result.release_value();

    // Load default theme from Ladybird resource root.
    auto theme_path = LexicalPath::join(
        StringView(WebView::s_ladybird_resource_root),
        StringView("themes", 6),
        StringView("Default.ini", 11)
    ).string();
    auto theme_result = Gfx::load_system_theme(theme_path);
    if (theme_result.is_error()) {
        ByteString msg = theme_result.error().string_literal();
        fprintf(stderr, "[pneuma-ladybird] load_system_theme failed: %s\n",
            msg.characters());
        delete browser;
        return nullptr;
    }
    auto theme = theme_result.release_value();

    // Explicit Web::DevicePixels constructor required.
    browser->view = WebView::HeadlessWebView::create(
        move(theme),
        Web::DevicePixelSize { Web::DevicePixels(width), Web::DevicePixels(height) }
    );
    if (!browser->view) {
        fprintf(stderr, "[pneuma-ladybird] HeadlessWebView::create returned null\n");
        delete browser;
        return nullptr;
    }

    browser->view->on_title_change = [browser](auto const& utf16_title) {
        auto utf8_string = utf16_title.to_utf8();
        browser->last_title_utf8 = ByteString(utf8_string.bytes_as_string_view());
        browser->title_seen = !browser->last_title_utf8.is_empty();
    };

    browser->view->on_load_finish = [browser](auto const& url) {
        (void)url;
        browser->load_complete = true;
    };

    // on_web_content_crashed takes no arguments.
    browser->view->on_web_content_crashed = [browser]() {
        browser->last_error = ByteString("WebContent process crashed");
        browser->load_failed = true;
        browser->load_complete = true;
    };

    return browser;
}

extern "C" int
pneuma_ladybird_navigate(
    PneumaLadybirdBrowser* browser,
    char const* url_cstr,
    int timeout_ms,
    char** out_title,
    char** out_error)
{
    if (!browser || !url_cstr || !out_title || !out_error)
        return PNEUMA_INVALID_ARG;

    *out_title = nullptr;
    *out_error = nullptr;

    // Validate URL before touching the event loop.
    auto parsed_url = URL::Parser::basic_parse(StringView(url_cstr, strlen(url_cstr)));
    if (!parsed_url.has_value()) {
        ByteString msg = ByteString::formatted("invalid URL: {}", url_cstr);
        *out_error = strdup(msg.characters());
        return PNEUMA_INVALID_ARG;
    }
    auto url = parsed_url.release_value();

    // Reset load state.
    browser->load_complete = false;
    browser->load_failed = false;
    browser->title_seen = false;
    browser->last_title_utf8 = {};
    browser->last_error = {};

    browser->view->load(url);

    // Pump event loop until load completes or timeout.
    auto deadline = std::chrono::steady_clock::now()
        + std::chrono::milliseconds(timeout_ms);
    auto& event_loop = Core::EventLoop::current();

    while (!browser->load_complete) {
        event_loop.pump(Core::EventLoop::WaitMode::PollForEvents);
        if (std::chrono::steady_clock::now() >= deadline) {
            *out_error = strdup("navigate timed out");
            return PNEUMA_TIMEOUT;
        }
    }

    // Title change can trail load_finish; pump a short grace period before returning.
    if (!browser->load_failed && !browser->title_seen) {
        auto title_deadline = std::chrono::steady_clock::now()
            + std::chrono::milliseconds(200);
        while (!browser->title_seen && std::chrono::steady_clock::now() < title_deadline)
            event_loop.pump(Core::EventLoop::WaitMode::PollForEvents);
    }

    if (browser->load_failed) {
        ByteString msg = browser->last_error.is_empty()
            ? ByteString("WebContent crashed")
            : browser->last_error;
        *out_error = strdup(msg.characters());
        return PNEUMA_RUNTIME_ERR;
    }

    *out_title = strdup(browser->last_title_utf8.characters());
    return PNEUMA_OK;
}

extern "C" void
pneuma_ladybird_free_string(char* ptr)
{
    free(ptr);
}

extern "C" void
pneuma_ladybird_browser_destroy(PneumaLadybirdBrowser* browser)
{
    if (!browser)
        return;
    browser->view = nullptr; // destroy view before app
    delete browser;
}
