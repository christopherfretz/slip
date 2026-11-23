// which location object, chosen from a name
#include "srcloc.h"
consteval std::source_location la() { return std::source_location::current(); }
consteval std::source_location lb() { return std::source_location::current(); }
consteval std::source_location pick(const char *p) { return p[0] == 'f' ? la() : lb(); }
void log(std::source_location);
void f(int) { log(pick(__func__)); }
