#include <LibCore/ElapsedTimer.h>

extern "C" int pneuma_ladybird_sanity_check()
{
    auto timer = Core::ElapsedTimer::start_new();
    (void)timer.elapsed_milliseconds();
    return 0xCAFE;
}
