#include "side.h"
// §10.1 AttrNode: `TLS_MODEL.a`: the fact differs.
#ifdef B
thread_local int v __attribute__((tls_model("initial-exec")));
#else
thread_local int v __attribute__((tls_model("local-exec")));
#endif
