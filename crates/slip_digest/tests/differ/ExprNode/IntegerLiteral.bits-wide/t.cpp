#include "side.h"
// §7 ExprNode `IntegerLiteral.bits-wide`.
#ifdef B
unsigned _BitInt(80) v = 0x1__uwb;
#else
unsigned _BitInt(80) v = 0x10000000000000000__uwb;
#endif
