// rev-112b probe: an immediate-escalating constexpr function template called in a default argument
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
template<class T> constexpr unsigned elen(T s) { return len(s); }
void log(unsigned = elen(__builtin_FUNCTION()));
void f() { log(); }
