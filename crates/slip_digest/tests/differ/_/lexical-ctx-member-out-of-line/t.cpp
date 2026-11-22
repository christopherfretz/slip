#include "side.h"
// §5.7 ColdDeclData `lexical_ctx`: a member defined outside its class (the global namespace, `Ref::none`).
#ifdef B
struct X { void m(); };
void X::m() {}
#else
struct X { void m() {} };
#endif
