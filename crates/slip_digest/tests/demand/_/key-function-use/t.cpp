#include "side.h"
// digest §14 (WP-045 review probe): a class with a key function, used.
struct S { virtual void f(); virtual ~S(); };
#ifdef B
void use(S& s) { s.f(); }
#endif
