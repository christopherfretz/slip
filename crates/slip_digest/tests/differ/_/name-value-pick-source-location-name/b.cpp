// WP-104 (digest §9), F1: which function's source-location name a compile-time choice returns
#include "srcloc.h"
consteval const char *la() { return std::source_location::current().function_name(); }
consteval const char *lb() { return std::source_location::current().function_name(); }
consteval const char *pick(const char *p) { return p[0] == 'f' ? la() : lb(); }
void take(const char *);
void start(int) { take(pick(__func__)); }
