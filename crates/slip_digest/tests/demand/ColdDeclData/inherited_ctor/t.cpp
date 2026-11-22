#include "side.h"
// §5.7 ColdDeclData `inherited_ctor` (demand, §11.5): a use of an inherited constructor leaves every unit equal.
struct A { A(int, int); };
struct D : A { using A::A; };
#ifdef B
void y() { D d(1, 2); }
#endif
