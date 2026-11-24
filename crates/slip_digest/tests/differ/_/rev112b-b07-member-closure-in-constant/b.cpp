// rev-112b probe: a closure in a static data member's initializer, the operand of an immediate invocation, whose body consumes its pretty name
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
consteval unsigned plen(const char *p = std::source_location::current().function_name()) { return len(p); }
constexpr unsigned lg(unsigned n = plen()) { return n; }
consteval auto pass(auto fn) { return fn; }
namespace acme {
struct S { static constexpr auto m = pass([] { return lg(); }); };
}
