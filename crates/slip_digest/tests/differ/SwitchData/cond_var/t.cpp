#include "side.h"
// §6 SwitchData `cond_var`.
#ifdef B
void f(int x) { switch (int y = x) {} }
#else
void f(int x) { switch (x) {} }
#endif
