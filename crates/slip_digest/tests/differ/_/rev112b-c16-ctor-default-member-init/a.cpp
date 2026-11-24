// rev-112b probe: a constructor's default argument used by a default member initializer: the use's function is the constructor of D (not renamed); control: f declares D
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
struct C { C(unsigned = len(__builtin_FUNCTION())); };
struct D { C c = C(); };
void f() { D d; C c2 = C(); }
