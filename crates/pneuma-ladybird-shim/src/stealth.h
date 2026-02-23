#pragma once

#include <AK/ByteString.h>
#include <AK/Optional.h>
#include <AK/StringView.h>

namespace Pneuma::Stealth {

Optional<ByteString> normalize_proxy_server(StringView raw_proxy);
void apply_proxy_environment(ByteString const& normalized_proxy);

}
