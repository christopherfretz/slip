#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): a conversion between a class and its base writes the base path, not the offset.
struct A { int a; };
#ifdef B
struct C { char c; long l; };
#else
struct C { char c; };
#endif
struct D : C, A { };
A *up(D *d) { return d; }
D *down(A *a) { return static_cast<D *>(a); }
