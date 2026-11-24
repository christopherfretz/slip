// rev-112b probe: a closure at namespace scope, the operand of an immediate invocation, whose body uses a default that consumes its pretty name (which holds the namespace)
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
consteval unsigned plen(const char *p = std::source_location::current().function_name()) { return len(p); }
constexpr unsigned lg(unsigned n = plen()) { return n; }
consteval auto pass(auto fn) { return fn; }
namespace acme {
inline constexpr auto k = pass([] { return lg(); });
}
