#include "side.h"
// WP-035, digest-kinds §7 `NewInitStyle` `Braces`.
#ifdef B
int *v = new int(1);
#else
int *v = new int{1};
#endif
