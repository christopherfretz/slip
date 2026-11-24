// rev-112b probe: a dependent default T::len(...) consumes at the instantiation's use
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
struct A { static consteval unsigned l(const char *s) { return len(s); } };
template<class T> void log(T, unsigned = T::l(__builtin_FUNCTION()));
void f() { log(A{}); }
