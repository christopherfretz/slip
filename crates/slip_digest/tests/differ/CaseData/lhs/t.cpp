#include "side.h"
// §6 CaseData `lhs`.
#ifdef B
void f(int x) { switch (x) { case 0 ... 3: ; } }
#else
void f(int x) { switch (x) { case 1 ... 3: ; } }
#endif
