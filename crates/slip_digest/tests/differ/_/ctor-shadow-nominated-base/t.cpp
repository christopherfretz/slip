#include "side.h"
// §5.7 ColdDeclData `ctor_shadow_nominated`: the constructor `A(int)` inherited by `C` through `B2`, which inherited it itself, against directly from its base `A` (WP-033 review, item 14).
struct A { A(int); };
struct B2 : A { using A::A; };
#ifdef B
struct C : B2, A { using A::A; };
#else
struct C : B2, A { using B2::B2; };
#endif
