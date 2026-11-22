#include "side.h"
// §5.3 DeclNode `NonTypeTemplateParm.data`: a pack (the head is in the key, §3.3).
#ifdef B
template <int... N> struct S;
#else
template <int N> struct S;
#endif
