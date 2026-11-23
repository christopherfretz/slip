#include "side.h"
// §5.7 ColdDeclData `hidden_shadow`: a using-declaration whose shadow a member of the class hides ([namespace.udecl]/14).
struct A { void f(); };
#ifdef B
struct D : A { using A::f; void f(); };
#else
struct D : A { using A::f; void f(int); };
#endif
