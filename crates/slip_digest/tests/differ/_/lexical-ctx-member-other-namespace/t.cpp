#include "side.h"
// §5.7 ColdDeclData `lexical_ctx`: a member defined outside its class, in the class's namespace against the global one.
namespace N { struct X { void m(); }; }
#ifdef B
namespace N { void X::m() {} }
#else
void N::X::m() {}
#endif
