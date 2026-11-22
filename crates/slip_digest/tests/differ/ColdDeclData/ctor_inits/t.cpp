#include "side.h"
// §5.7 ColdDeclData `ctor_inits`: a member initialiser list against none.
#ifdef B
struct S { int a; S() : a(1) {} };
#else
struct S { int a; S() {} };
#endif
