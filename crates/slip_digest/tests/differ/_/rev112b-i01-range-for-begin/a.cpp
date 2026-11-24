// rev-112b probe: a range-for's implicit begin() call uses a default that consumes the caller's name (clang: the length of the caller's name)
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
struct R { int a[1]; const int *begin(unsigned n = len(__builtin_FUNCTION())) const; const int *end() const; };
void f() { for (int x : R{{7}}) take(x); }
