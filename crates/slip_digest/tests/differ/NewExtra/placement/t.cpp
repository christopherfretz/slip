#include "side.h"
// WP-035, digest-kinds §7 `NewExtra` `placement`.
void *operator new(decltype(sizeof 0), void *) noexcept;
char buf[8];
#ifdef B
int *v = new int;
#else
int *v = new (buf) int;
#endif
