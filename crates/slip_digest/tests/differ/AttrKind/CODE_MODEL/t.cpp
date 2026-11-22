#include "side.h"
// §10.1 AttrKind: `CODE_MODEL`: the attribute on one side only.
#ifdef B
int v __attribute__((model("small")));
#else
int v;
#endif
