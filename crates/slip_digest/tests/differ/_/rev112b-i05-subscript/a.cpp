// std: c++23
// rev-112b probe: a C++23 multi-parameter operator[] with a consuming default
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
struct V { int operator[](int, unsigned = len(__builtin_FUNCTION())) const; };
void f() { V v; take(v[0]); }
