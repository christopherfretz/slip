#include "side.h"
// §10.1 AttrSyntax: `Keyword`: the fact differs.
#ifdef B
alignas(8) int v;
#else
__attribute__((aligned(8))) int v;
#endif
