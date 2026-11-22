#include "side.h"
// §5.7 ColdDeclData `ctor_shadow_nominated`: an inherited constructor reached through a base that inherited it itself.
struct A { A(int); };
#ifdef B
struct B2 : A { using A::A; };
#else
struct B2 : A { B2(int); };
#endif
struct C : B2 { using B2::B2; };
