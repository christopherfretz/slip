#include "side.h"
// WP-035, digest-kinds §7 `NewInitStyle` `None`.
#ifdef B
int *v = new int();
#else
int *v = new int;
#endif
