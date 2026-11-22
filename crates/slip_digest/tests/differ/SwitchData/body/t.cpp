#include "side.h"
// §6 SwitchData `body`.
#ifdef B
void f(int x) { switch (x) { ; } }
#else
void f(int x) { switch (x) {} }
#endif
