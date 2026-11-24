// WP-112 second fix round, G1 (digest §9 "Defaults at a use" (c)): std::source_location::current() inside the immediate invocation len(...) in a default argument and a default member initializer: two invocations above the builtin, so it is consumed and the caller's name written by value (clang: 16 against 14 characters)
#include "srcloc.h"
void takeu(unsigned);
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void log(unsigned = len(std::source_location::current().function_name()));
struct Agg { int x; unsigned n = len(std::source_location::current().function_name()); };
namespace legacy {
void f() { log(); }
void g() { Agg a{1}; takeu(a.n); }
}
