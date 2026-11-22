// std: c++17
#include "side.h"
// §6 SwitchData `init`.
#ifdef B
void f(int x) { switch (int y = 0; x) {} }
#else
void f(int x) { switch (x) {} }
#endif
