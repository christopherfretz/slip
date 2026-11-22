#include "side.h"
// §7.4 ColdExprData `union_field`.
// a union's parenthesised aggregate initialisation
union U { int i; float f; };
#ifdef B
U v{1};
#else
U v(1);
#endif
