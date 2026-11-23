// std: c++17
#include "side.h"
// §6 IfData `init`: an `if` with and without an init-statement (WP-034 extra test).
#ifdef B
void f(int x) { if (int y = 0; x) ; }
#else
void f(int x) { if (x) ; }
#endif
