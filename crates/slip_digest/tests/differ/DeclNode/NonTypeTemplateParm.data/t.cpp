#include "side.h"
// §5.3 DeclNode `NonTypeTemplateParm.data`: a pack.
#ifdef B
template <int... N> struct S;
#else
template <int N> struct S;
#endif
