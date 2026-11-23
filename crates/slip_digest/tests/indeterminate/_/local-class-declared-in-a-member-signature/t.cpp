#include "side.h"
// digest §4.3: `struct Q *` in a local class's member signature declares Q
// in f's block ([basic.scope.pdecl]/7), and the later `struct Q {}`
// redeclares it; Kiln makes them two classes, and the first is in no
// declaration statement, so f's numbering does not find it: `UndeclaredLocal`.
void f() { struct A { A *p; void g(struct Q *); }; struct Q { long m; }; }
