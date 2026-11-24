// rev-112b probe: control for b06: the same closure with no immediate invocation around it
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
consteval unsigned plen(const char *p = std::source_location::current().function_name()) { return len(p); }
constexpr unsigned lg(unsigned n = plen()) { return n; }
consteval auto pass(auto fn) { return fn; }
namespace legacy {
inline constexpr auto k = [] { return lg(); };
}
