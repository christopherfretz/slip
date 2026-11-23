// WP-104 (digest §9), F1: which of two call sites' default-argument locations a compile-time choice returns
#include "srcloc.h"
consteval std::source_location here(std::source_location l = std::source_location::current()) { return l; }
consteval std::source_location pick(const char *p) { return p[0] == 'f' ? here() : here(); }
void log(std::source_location);
void start(int) { log(pick(__func__)); }
