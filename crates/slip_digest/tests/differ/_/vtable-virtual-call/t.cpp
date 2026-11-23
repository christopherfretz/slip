#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): a virtual call stores no vtable index: the class's order of virtual functions decides it.
#ifdef B
struct A { virtual void g(); virtual void f(); };
#else
struct A { virtual void f(); virtual void g(); };
#endif
void call(A *a) { a->f(); }
