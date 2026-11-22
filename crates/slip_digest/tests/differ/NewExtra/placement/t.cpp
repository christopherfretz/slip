#include "side.h"
// WP-035, digest-kinds §7 `NewExtra` `placement`: the same allocation function, another placement argument.
void *operator new(decltype(sizeof 0), void *) noexcept;
char buf[8], buf2[8];
#ifdef B
int *v = new (buf2) int;
#else
int *v = new (buf) int;
#endif
