// rev-112b probe: control for c38: #line names another file, so the __impl's function name is its own object
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
#line 10 "other.cpp"
void log(unsigned = len(std::source_location::current().function_name()));
void f() { log(); }
