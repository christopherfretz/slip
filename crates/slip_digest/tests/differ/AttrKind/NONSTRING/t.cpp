#include "side.h"
// §10.1 AttrKind: `NONSTRING`: the attribute on one side only.
#ifdef B
char s[4] __attribute__((nonstring));
#else
char s[4];
#endif
