#include "side.h"
// §10.1 AttrNode: `CODE_MODEL.a`: the fact differs.
#ifdef B
int v __attribute__((model("large")));
#else
int v __attribute__((model("small")));
#endif
