#include "side.h"
// WP-035, digest-kinds §7 `NewExtra` `init`.
#ifdef B
int *v = new int(2);
#else
int *v = new int(1);
#endif
