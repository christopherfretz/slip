#include "side.h"
// §7 ExprNode `IntegerLiteral.a-wide`.
#ifdef B
unsigned _BitInt(80) v = 0x20000000000000000__uwb;
#else
unsigned _BitInt(80) v = 0x10000000000000000__uwb;
#endif
