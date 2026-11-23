#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): a `dynamic_cast` writes its type; the path is found at run time.
struct A { virtual ~A(); int a; };
#ifdef B
struct C { virtual ~C(); char c; long l; };
#else
struct C { virtual ~C(); char c; };
#endif
struct D : C, A { };
D *down(A *a) { return dynamic_cast<D *>(a); }
