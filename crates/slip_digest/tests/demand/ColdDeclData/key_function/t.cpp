#include "side.h"
// §5.7 ColdDeclData `key_function` (demand, §11.5): a use that needs the vtable leaves every unit equal.
struct S { virtual void f(); virtual void g() {} };
void S::f() {}
#ifdef B
void y() { S s; s.g(); }
#endif
