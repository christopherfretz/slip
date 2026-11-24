// rev-112b probe: a template argument g() whose default holds an immediate invocation of the builtin
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
constexpr unsigned g(unsigned n = len(__builtin_FUNCTION())) { return n; }
template<unsigned N> void t();
void f() { t<g()>(); }
