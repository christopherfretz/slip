#include "side.h"
// §5.7 ColdDeclData `lib_builtin` (demand, §11.6): a call of a declared library function leaves every unit equal.
extern "C" int abs(int);
#ifdef B
int y() { return abs(-1); }
#endif
