// WP-104 (digest §9), consumed: the function_name() of a constexpr source_location in a static_assert
#include "srcloc.h"
constexpr unsigned long len(const char *p) { unsigned long n = 0; while (p[n]) ++n; return n; }
namespace legacy {
void f() { constexpr auto l = std::source_location::current(); static_assert(len(l.function_name()) > 3); }
}
