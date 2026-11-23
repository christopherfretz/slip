#include "side.h"
// §6 CaseData `rhs`: a GNU case range.
#ifdef B
void f(int x) { switch (x) { case 1 ... 3: ; } }
#else
void f(int x) { switch (x) { case 1: ; } }
#endif
