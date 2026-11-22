#include "side.h"
// WP-035, digest-kinds §7 `NewExtra` `array_size`.
#ifdef B
int *v = new int[3];
#else
int *v = new int[2];
#endif
