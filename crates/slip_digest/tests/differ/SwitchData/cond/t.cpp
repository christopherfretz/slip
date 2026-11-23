#include "side.h"
// §6 SwitchData `cond`.
#ifdef B
void f(int x, int y) { switch (y) {} }
#else
void f(int x, int y) { switch (x) {} }
#endif
