// WP-145 item 1: two callers in one unit, each under a #line that names the file as that caller's pretty name (the nearest reachable collision: a real file cannot be named so); renaming one moves only its unit
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void log(unsigned = len(std::source_location::current().function_name()));
#line 10 "void g()"
void g() { log(); }
#line 20 "void start()"
void start() { log(); }
