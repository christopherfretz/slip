#include "side.h"
// §5.7 ColdDeclData `friends`: the class's list of friend declarations.
void f();
void g();
#ifdef B
struct X { friend void g(); };
#else
struct X { friend void f(); };
#endif
