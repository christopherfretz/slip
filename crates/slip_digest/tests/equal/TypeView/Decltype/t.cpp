#include "side.h"
// §4.3 TypeView `Decltype`: sugar (§4.2 rule 7). An operand that names
// nothing is not written (digest §2.4, S5); one that names something is
// an operand unit (tests/operands/_/typeview-decltype).
#ifdef B
decltype(0) v;
#else
int v;
#endif
