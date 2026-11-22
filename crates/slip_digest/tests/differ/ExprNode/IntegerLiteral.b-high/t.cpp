#include "side.h"
// §7 ExprNode `IntegerLiteral.b-high`.
#ifdef B
long v = 0x200000001;
#else
long v = 0x100000001;
#endif
