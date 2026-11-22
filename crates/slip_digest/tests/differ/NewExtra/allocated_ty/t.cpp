#include "side.h"
// WP-035, digest-kinds §7 `NewExtra` `allocated_ty`.
#ifdef B
void *v = new long;
#else
void *v = new int;
#endif
