// rev-112b probe: #line names the file exactly as f's pretty name, so Kiln's one string object per content makes the __impl's function name the file's
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
#line 10 "void f()"
void log(unsigned = len(std::source_location::current().function_name()));
void f() { log(); }
