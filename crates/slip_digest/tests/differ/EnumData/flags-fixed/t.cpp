#include "side.h"
// §5.5 EnumData `flags-fixed`: a fixed underlying type against none.
#ifdef B
enum E : int { a };
#else
enum E { a };
#endif
