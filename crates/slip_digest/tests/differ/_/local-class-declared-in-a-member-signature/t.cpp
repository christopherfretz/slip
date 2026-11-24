#include "side.h"
// digest §4.3: `struct Q *` in a local class's member signature declares Q
// in f's block ([basic.scope.pdecl]/7), and the later `struct Q {}`
// redeclares it: since Kiln WP-2474 one class, which f's numbering finds
// (it was two, the first in no declaration statement: `UndeclaredLocal`).
#ifdef B
void f() { struct A { A *p; void g(struct Q *); }; struct Q { long m; }; }
#else
void f() { struct A { A *p; void g(struct Q *); }; struct Q { int m; }; }
#endif
