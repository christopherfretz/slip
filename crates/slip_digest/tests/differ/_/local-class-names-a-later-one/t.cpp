#include "side.h"
// §4.3: a local class naming itself and a later local class, declared
// before it, in a member's signature. (Declared there by an
// elaborated-type-specifier, Kiln makes it another class than the later
// definition: indeterminate/_/local-class-declared-in-a-member-signature.)
#ifdef B
void f() { struct Q; struct A { A *p; void g(Q *); }; struct Q { long m; }; }
#else
void f() { struct Q; struct A { A *p; void g(Q *); }; struct Q { int m; }; }
#endif
