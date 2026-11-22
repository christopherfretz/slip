#include "side.h"
// §10.1 AttrKind: `TLS_MODEL`: the attribute on one side only.
#ifdef B
thread_local int v __attribute__((tls_model("local-exec")));
#else
thread_local int v;
#endif
