#include "stealth.h"

#include <cstdlib>

namespace Pneuma::Stealth {

Optional<ByteString> normalize_proxy_server(StringView raw_proxy)
{
    auto trimmed = raw_proxy.trim_whitespace();
    if (trimmed.is_empty())
        return {};

    auto authority = trimmed;
    if (auto scheme_sep = authority.find("://"sv); scheme_sep.has_value())
        authority = authority.substring_view(*scheme_sep + 3);

    if (auto path_sep = authority.find('/'); path_sep.has_value())
        authority = authority.substring_view(0, *path_sep);

    authority = authority.trim_whitespace();
    if (authority.is_empty())
        return {};

    return ByteString(authority);
}

void apply_proxy_environment(ByteString const& normalized_proxy)
{
    auto proxy_url = ByteString::formatted("http://{}", normalized_proxy);
    setenv("http_proxy", proxy_url.characters(), 1);
    setenv("https_proxy", proxy_url.characters(), 1);
    setenv("HTTP_PROXY", proxy_url.characters(), 1);
    setenv("HTTPS_PROXY", proxy_url.characters(), 1);
}

}
