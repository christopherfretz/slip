#include "side.h"
// §6 CaseData `sub`.
#ifdef B
void f(int x) { switch (x) { case 1: break; } }
#else
void f(int x) { switch (x) { case 1: ; } }
#endif
