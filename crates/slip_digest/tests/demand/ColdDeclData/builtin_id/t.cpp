#include "side.h"
// §5.7 ColdDeclData `builtin_id` (demand, §11.6): a use of a builtin Kiln declares on first use leaves every unit equal.
int a = 1;
#ifdef B
int y() { return __builtin_abs(-1); }
#endif
