#include "side.h"
// §5.5 EnumData `flags-fixed`: a fixed underlying type against none, the same type (int) and the same promotion type on both sides.
#ifdef B
enum E : int { a = -1 };
#else
enum E { a = -1 };
#endif
