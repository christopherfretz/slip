// WP-104 (digest §9), F1: the plain or the pretty source-location name of one function, chosen from a name
#include "srcloc.h"
consteval const char *n(bool b) { return b ? __builtin_FUNCTION() : std::source_location::current().function_name(); }
consteval const char *pick(const char *p) { return n(p[0] == 'f'); }
void take(const char *);
void start(int) { take(pick(__func__)); }
