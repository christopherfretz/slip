// rev-112b probe: a class template member's default used from two instantiations
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
template<class T> struct W { static void log(unsigned = len(__builtin_FUNCTION())); };
void other() { W<int>::log(); }
void f() { W<char>::log(); W<int>::log(); }
